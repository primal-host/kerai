# Plan 06: Distribution

*Depends on: Plan 04 (CRDT Operations), Plan 05 (CLI)*
*Enables: Plan 09 (Agent Swarms), Plan 10 (ZK Marketplace)*

## Goal

Enable sharing kerai databases between developers and machines. At the end of this plan, you can clone a project, push changes, pull changes, and sync between local and remote Postgres instances.

## Two Sync Modes

### Full Snapshot (the "clone")

For initial setup or full state transfer. Uses `pg_dump`/`pg_restore`.

```bash
# Export a snapshot
kerai snapshot export myproject-2026-02-15.dump

# Import a snapshot into a local database
kerai snapshot import myproject-2026-02-15.dump

# Clone: download snapshot from a remote and import
kerai clone kerai://gitea.primal.host/billy/myproject
```

Under the hood, `kerai clone`:
1. Fetches the latest `pg_dump` from the remote (stored as an artifact, served over HTTP)
2. Creates a local Postgres database (starting a container if needed)
3. Runs `pg_restore` to populate it
4. Writes `.kerai/config.toml` with both local and remote connection profiles

### Incremental Sync (the "push/pull")

For ongoing collaboration. Exchanges CRDT operations since the last sync point.

```bash
# Push local operations to the remote
kerai push

# Pull remote operations into local database
kerai pull

# Two-way sync (push then pull)
kerai sync
```

**How incremental sync works:**

1. Compare version vectors between local and remote
2. Identify operations the remote has that local doesn't (and vice versa)
3. Export those operations as a batch
4. Import and apply them through the CRDT operation engine (Plan 04)
5. Update the version vector on both sides

Because operations commute (Plan 04), the order of application doesn't matter. There are no merge conflicts. Push and pull are symmetric operations.

## Deliverables

### 6.1 Snapshot Export/Import

```bash
# Uses pg_dump with custom format, parallel workers
kerai snapshot export [--output path] [--parallel N]

# Uses pg_restore, parallel workers
kerai snapshot import <path> [--parallel N] [--db connection-string]
```

The snapshot format is just a `pg_dump -Fc` file. No custom format needed. Postgres's native tooling handles compression, parallelism, and cross-version compatibility.

### 6.2 Operation Export/Import

```bash
# Export operations since a given version vector
kerai ops export --since '{"billy": 100}' --output ops.jsonl

# Import operations from a file
kerai ops import ops.jsonl
```

The operation exchange format is newline-delimited JSON (JSONL):

```jsonl
{"op_type":"node_insert","node_id":"...","author":"billy","lamport_ts":148,"author_seq":148,"payload":{...}}
{"op_type":"node_update","node_id":"...","author":"billy","lamport_ts":149,"author_seq":149,"payload":{...}}
```

JSONL is human-readable, streamable, and trivial to parse. Operations are self-describing — each line contains everything needed to apply it.

### 6.3 Remote Registry

Kerai needs to know where to push/pull. Proposed: use existing Git hosting as the transport layer (for now).

```toml
# .kerai/config.toml
[remote "origin"]
url = "kerai://gitea.primal.host/billy/myproject"
```

The `kerai://` protocol is HTTP(S) under the hood:
- `GET /snapshot/latest` — download the latest pg_dump
- `GET /ops?since=<version-vector>` — download operations since a version vector
- `POST /ops` — upload new operations
- `GET /version` — get the remote's current version vector

This could be served by a simple HTTP server, a Gitea plugin, or even a static file host for snapshot-only workflows.

### 6.4 Selective Sync

Leverage the AST structure for partial clones — the monorepo split/join problem:

```bash
# Clone only a specific package subtree
kerai clone kerai://remote/bigproject --package pkg/auth

# Push only changes within a package
kerai push --package pkg/auth
```

This exports/imports only the nodes, edges, and operations under the specified subtree. The version vector is filtered to relevant authors/operations.

### 6.5 Shared Server Mode

When multiple developers point at the same Postgres instance, there's nothing to sync — they're already reading the same data. MVCC provides isolation.

```bash
# Developer A and Developer B both use:
kerai --db postgres://team-server:5432/myproject commit -m "my changes"
```

No push, no pull, no sync. Operations go directly into the shared database. This is the simplest mode and the one that scales most naturally to agent swarms (Plan 09).

### 6.6 Background Workers (pgrx)

The pgrx extension runs background workers inside Postgres for continuous sync operations:

- **Peer sync worker:** Periodically exchanges operations with known peers. Compares version vectors, fetches missing ops, applies them through the CRDT engine.
- **Peer discovery worker:** Contacts bootstrap nodes to discover new instances. Registers them in the `instances` table.
- **Health check worker:** Monitors peer connectivity, updates `instances.last_seen`.

```sql
-- Join the network via a bootstrap node
SELECT kerai.join_network('bootstrap.ker.ai');

-- List discovered peers
SELECT * FROM kerai.peers();

-- Manually trigger sync with a specific peer
SELECT kerai.sync('research-server');

-- Check what we're missing from a peer
SELECT * FROM kerai.vector_diff('research-server');
```

Background workers are started by `CREATE EXTENSION kerai` but remain idle until peers are configured. They activate on `kerai.join_network()` or when remote connection profiles are added.

## Decisions to Make

- **Snapshot hosting:** Where do pg_dump files live? Options: alongside the Gitea repo (as release artifacts), on S3/MinIO, or served by a dedicated kerai server. Proposed: start with Gitea release artifacts, evolve to dedicated server later.
- **Authentication:** Use the same auth as the remote host (Gitea tokens, SSH keys). Don't invent a new auth system.
- **Conflict-free guarantee at scale:** With many agents pushing ops, the ops table grows fast. Need to verify that Postgres handles high-throughput inserts to the operations table. Proposed: benchmark with 1K ops/sec as target, batch inserts for efficiency.
- **Bootstrap nodes:** What are the well-known entry points? Proposed: start with a single bootstrap node (`bootstrap.ker.ai`) that maintains a peer list. Instances announce themselves and discover others. No central authority — the bootstrap is just an introduction service.
- **Sync protocol:** HTTP/2 or direct Postgres connections (via `postgres_fdw` or `dblink`)? Proposed: HTTP/2 for op exchange (firewall-friendly, cacheable), `postgres_fdw` for ad-hoc cross-instance queries (real-time, SQL-native).

## Out of Scope

- Building the HTTP server for the remote registry (a separate service, possibly integrated with Gitea)
- Peer-to-peer sync without a central server (possible with CRDTs but adds complexity)
- Access control / permissions (who can push to what package)
