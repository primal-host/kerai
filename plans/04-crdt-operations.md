# Plan 04: CRDT Operations

*Depends on: Plan 01 (Foundation)*
*Enables: Plan 05 (CLI), Plan 06 (Distribution), Plan 08 (AI Perspectives), Plan 09 (Agent Swarms)*

## Goal

Define the operation model that makes concurrent editing deterministic. At the end of this plan, two independent writers can modify the same database (or separate databases) and arrive at identical state without coordination. This is the mathematical core of kerai.

## Why CRDTs

Git merges are heuristic — they guess the right answer and ask humans when they're unsure. CRDTs (Conflict-free Replicated Data Types) guarantee convergence by construction. Every operation is designed so that applying the same set of operations in any order produces the same result. No conflicts. No merge algorithms. No human intervention.

For a system targeting a million concurrent agents, anything less than this guarantee is unworkable.

## Deliverables

### 4.1 Operation Types

Every change to the database is recorded as an operation. Operations are the atoms of the system — they're what gets stored, synced, and replayed.

```sql
CREATE TABLE operations (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    instance_id     uuid NOT NULL REFERENCES instances(id), -- originating instance (provenance)
    op_type         text NOT NULL,        -- see below
    node_id         uuid NOT NULL,
    author          text NOT NULL,
    lamport_ts      bigint NOT NULL,      -- logical timestamp
    author_seq      bigint NOT NULL,      -- per-author sequence number
    payload         jsonb NOT NULL,       -- operation-specific data
    signature       bytea NOT NULL,       -- Ed25519 signature (same as versions in Plan 01)
    created_at      timestamptz DEFAULT now()
);

CREATE INDEX idx_ops_node ON operations(node_id);
CREATE INDEX idx_ops_instance ON operations(instance_id);
CREATE INDEX idx_ops_author_seq ON operations(author, author_seq);
CREATE INDEX idx_ops_lamport ON operations(lamport_ts);
```

**Note on `versions` (Plan 01):** The `operations` table is the source of truth. The `nodes`, `edges`, and `versions` tables from Plan 01 are materialized views rebuilt by replaying operations (see section 4.5). The `versions` table in Plan 01 captures the same information in a denormalized, query-optimized form.

**Operation types:**

| op_type | payload | Effect |
|---------|---------|--------|
| `node_insert` | `{kind, content, parent_id, position, metadata}` | Create a new node |
| `node_delete` | `{}` | Mark node as deleted (tombstone) |
| `node_move` | `{new_parent_id, new_position}` | Reparent or reorder a node |
| `node_update` | `{field, old_value, new_value}` | Change content or metadata |
| `edge_insert` | `{source_id, target_id, relation, metadata}` | Create a relationship |
| `edge_delete` | `{}` | Remove a relationship |

### 4.2 Causal Identity

Every operation is uniquely identified by `(author, author_seq)`. This pair is the causal ID — it tells you exactly who created this operation and where it falls in their sequence.

The `lamport_ts` is a logical clock:
- When creating an operation: `lamport_ts = max(all_seen_timestamps) + 1`
- This ensures a total order that respects causality without requiring synchronized clocks

### 4.3 Version Vectors

The state of any database is summarized by its version vector — a map from author to the highest `author_seq` seen from that author:

```sql
CREATE TABLE version_vector (
    author      text PRIMARY KEY,
    max_seq     bigint NOT NULL
);
```

**Properties:**
- Two databases with identical version vectors have identical state
- Comparing two version vectors instantly reveals what's missing: for each author, the difference in `max_seq` tells you how many ops to send
- Merging is: take the max of each author's seq across both vectors

### 4.4 Commutativity Rules

Operations must commute — applying op A then op B must give the same result as B then A. This requires rules for every conflict case:

**Concurrent inserts at the same position:** Order by `(lamport_ts, author)` as tiebreaker. Deterministic because both sides use the same tiebreaker.

**Concurrent delete and update of the same node:** Delete wins. The update is applied but the node remains tombstoned. (This matches "last writer wins" with delete as a special case.)

**Concurrent moves of the same node to different parents:** Higher `lamport_ts` wins. Ties broken by `author` lexicographic order.

**Concurrent moves creating a cycle:** Detect cycles after applying moves. If a cycle is detected, the move with the lower `lamport_ts` is reverted to break the cycle.

### 4.5 Materialized State

The `nodes`, `edges`, and `versions` tables from Plan 01 become *materialized views* of the operation log. They can be rebuilt at any time by replaying operations from the beginning.

In practice, we maintain them incrementally:
- When an operation is applied, update the materialized tables immediately
- The `operations` table is the source of truth
- The materialized tables are the query-optimized view

### 4.6 Operation Application Engine

A Rust module (part of the pgrx extension) that:

1. Accepts an operation (via SQL function call or internal API)
2. Validates it (does the referenced node exist? is the operation well-formed?)
3. Verifies the Ed25519 signature (via `ed25519-dalek`)
4. Applies commutativity rules if there's a conflict
5. Updates the materialized tables
6. Updates the version vector
7. Returns the resulting state change

This engine is the core of kerai. Everything else — parsing, reconstruction, CLI, sync — calls into it. Because it runs inside Postgres via pgrx, it has direct access to the data without network overhead.

```sql
-- Apply an operation via the DSL
SELECT kerai.apply_op(
    op_type := 'node_update',
    node_id := 'abc123...',
    payload := '{"field": "content", "old_value": "foo", "new_value": "bar"}'
);

-- Get the current version vector
SELECT * FROM kerai.version_vector();

-- Compare version vectors with a remote instance
SELECT * FROM kerai.vector_diff('research-server');
```

## Decisions to Make

- **Tombstones vs hard deletes:** Proposed: tombstones (mark as deleted, don't remove the row). This preserves history and makes undo possible. Garbage collection of ancient tombstones can happen later.
- **Operation compaction:** Over time, the operation log grows without bound. Can we compact it? Proposed: yes, but only operations older than a configurable threshold. Recent ops must remain for sync. This is analogous to git's garbage collection.
- **Tree move semantics:** The "concurrent moves creating cycles" problem is the hardest part of tree CRDTs. Proposed: use the CRDT tree algorithm from Martin Kleppmann's research (see references). This is well-studied.

## References

- Kleppmann, M. et al. "A highly-available move operation for replicated trees" (2021) — the definitive paper on tree CRDTs with move operations
- Gritzko's RDX: https://github.com/gritzko/librdx — the merge semantics kerai draws from
- Shapiro, M. et al. "Conflict-free Replicated Data Types" (2011) — foundational CRDT paper

## Out of Scope

- Network sync protocol (Plan 06)
- Applying operations from parsed source (that's the parser + this engine working together)
- UI for conflict visualization (we assert there are no conflicts, by design)
