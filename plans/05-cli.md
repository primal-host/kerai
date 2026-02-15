# Plan 05: CLI

*Depends on: Plan 02 (Rust Parser), Plan 03 (Source Reconstruction), Plan 04 (CRDT Operations)*
*Enables: Plan 06 (Distribution), Plan 07 (Code Queries), Plan 11 (External Economy)*

## Goal

Build the `kerai` command-line tool — a thin Rust client that calls into the pgrx extension's DSL functions. The CLI provides a familiar VCS-like interface for developers while exposing the database-native capabilities that make kerai different. The heavy lifting happens inside the extension; the CLI is the human-friendly wrapper.

## Design Principle

The CLI connects to Postgres via a connection string. It does not know or care whether the database is local or remote. Because the logic lives in the pgrx extension (not the CLI), any Postgres client is technically a kerai client — the CLI just makes it ergonomic.

```
~/.config/kerai/config.toml

[default]
connection = "postgres://localhost:5432/myproject"

[profiles.team]
connection = "postgres://kerai.internal:5432/myproject"
```

Switch profiles: `kerai --profile team status`

## Deliverables

### 5.1 Project Initialization

```bash
# Initialize a new kerai project from an existing Rust crate
kerai init .

# What this does:
# 1. Reads connection string from config (or --db flag)
# 2. Connects to Postgres, runs CREATE EXTENSION kerai (if not already loaded)
# 3. The extension creates the schema, generates keypair, creates instance record
# 4. CLI calls kerai.parse_crate() to ingest the Rust source (Plan 02)
# 5. Records the initial state as operations (Plan 04)
# 6. Creates .kerai/config.toml in the project root
```

### 5.2 Working Directory Sync

The "working directory" is a conventional directory of `.rs` files, just like today. Developers edit text files. Kerai bridges between the text world and the database world.

```bash
# Reconstruct source files from the database
kerai checkout [--file path] [--module name]

# Parse changed source files back into the database, recording operations
kerai commit -m "refactored auth handler"

# Show what's changed between the working directory and the database
kerai status

# Show structural diff (not line diff) between working dir and database
kerai diff
```

**`kerai status`** compares the filesystem against the database:
- Re-parses changed files into a temporary AST
- Diffs against the stored AST node-by-node
- Reports changes at the structural level: "function validate_token: body modified", "new function parse_header added"

**`kerai diff`** shows structural changes:
```
src/auth/handler.rs:
  ~ ItemFn validate_token
    + ExprIf (line 47)
    ~ ExprReturn (line 52): changed return value
  + ItemFn parse_header (new)
```

### 5.3 History

```bash
# Show version history (from operations table)
kerai log [--node path] [--author name] [--since date]

# Show the version vector (current state identity)
kerai version

# Show the state of a node at a specific version vector
kerai show <node-path> [--at <version-vector>]
```

### 5.4 Direct Database Access

For power users and agents — query the database directly through the CLI:

```bash
# Run a SQL query against the kerai database
kerai query "SELECT kind, content FROM nodes WHERE path <@ 'crate.auth'"

# Use DSL functions directly
kerai query "SELECT * FROM kerai.find('auth.validate_token', callers := true)"

# Shorthand for common structural queries
kerai find --kind ItemFn --module auth
kerai refs --to validate_token
kerai callers validate_token
kerai callees validate_token
```

### 5.5 Connection Management

```bash
# Show current connection info and database status
kerai info

# Test connection and verify extension is loaded
kerai ping

# Switch connection profile
kerai use <profile-name>
```

### 5.6 Docker Integration

For local development, kerai can manage its own Postgres container:

```bash
# Start a local Postgres container with the kerai extension loaded
kerai db start

# Stop the local container (data persists on volume)
kerai db stop

# Destroy the local container and volume
kerai db destroy

# Show container status
kerai db status
```

This uses the naming convention from CLAUDE.md: container and image named `primal-kerai` (or derived from the project path). The volume name matches.

## Decisions to Make

- **CLI framework:** `clap` (Rust) is the natural choice — we're already in Rust for the pgrx extension, and `clap` is the ecosystem standard. Single static binary.
- **Config format:** TOML for simplicity. Stored in `~/.config/kerai/config.toml` (global) and `.kerai/config.toml` (per-project).
- **Working directory convention:** Should kerai manage a shadow directory (like `.git`), or should the source files live alongside the config? Proposed: source files are in the normal project directory. `.kerai/` contains only config, not data — the data is in Postgres.
- **CLI ↔ Extension boundary:** The CLI should be as thin as possible. All logic (parsing, reconstruction, CRDT ops, market operations) lives in the extension. The CLI formats output, manages connection profiles, and provides the developer-friendly command structure. If something can be a SQL function call, it should be.

## Out of Scope

- Remote sync / push / pull (Plan 06)
- Advanced code queries (Plan 07)
- Agent-specific CLI commands (Plan 09)
