# Plan 07: Code Queries

*Depends on: Plan 05 (CLI)*
*Enables: Plan 08 (AI Perspectives)*

## Goal

Expose the full power of the relational model to developers and agents querying the codebase. This is the "SCM as a database" promise — querying code structure and history with the same fluency you'd query any database.

## Why This Matters

Git's query capabilities are: `git log`, `git grep`, `git blame`. All text-based, all line-oriented. Kerai stores *structure*, so queries can be structural:

- "What functions were added to the auth module this week?"
- "Who are the callers of validate_token?"
- "Show me every function whose signature changed between version A and version B"
- "What's the most-modified function in the last month?"

These are JOIN queries across `nodes`, `edges`, `versions`, and `operations`. SQL already handles them — this plan is about making them accessible.

## Deliverables

### 7.1 Built-in Query Commands

Shorthand commands for common structural queries:

```bash
# Find nodes by kind and location
kerai find --kind ItemFn --module auth
kerai find --kind ExprIf --file handler.rs

# Cross-reference queries
kerai refs validate_token              # all references to this identifier
kerai callers validate_token           # functions that call this function
kerai callees validate_token           # functions this function calls
kerai imports auth                     # what modules does auth import?
kerai importers auth                   # what modules import auth?
kerai implements Display               # what types implement this trait?

# History queries
kerai history validate_token           # all changes to this function
kerai history --author agent-1         # all changes by this author
kerai history --since 2026-02-01       # all changes since a date
kerai blame validate_token             # per-node attribution (not per-line)

# Structural statistics
kerai stats                           # node counts by kind, module sizes
kerai hotspots --since 2026-01-01     # most-modified nodes
kerai complexity --module auth        # tree depth, node count per function
```

### 7.2 Raw SQL Access

For queries that don't have a shorthand:

```bash
# Direct SQL query with formatted output
kerai query "
  SELECT n.path, count(e.id) as caller_count
  FROM nodes n
  JOIN edges e ON e.target_id = n.id AND e.relation = 'calls'
  WHERE n.kind = 'ItemFn'
  GROUP BY n.path
  ORDER BY caller_count DESC
  LIMIT 10
"

# Output formats
kerai query --format table "..."    # default: ASCII table
kerai query --format json  "..."    # JSON array of objects
kerai query --format csv   "..."    # CSV
```

### 7.3 Saved Queries

Store reusable queries in the database or in config:

```toml
# .kerai/queries.toml
[queries.dead-code]
description = "Functions with no callers"
sql = """
  SELECT n.path, n.content
  FROM nodes n
  LEFT JOIN edges e ON e.target_id = n.id AND e.relation = 'calls'
  WHERE n.kind = 'ItemFn'
  AND n.metadata->>'visibility' = 'pub'
  AND e.id IS NULL
"""

[queries.recent-changes]
description = "Functions modified in the last 7 days"
sql = """
  SELECT DISTINCT n.path
  FROM nodes n
  JOIN operations o ON o.node_id = n.id
  WHERE n.kind = 'ItemFn'
  AND o.created_at > now() - interval '7 days'
"""
```

```bash
kerai query --saved dead-code
kerai query --saved recent-changes
```

### 7.4 Temporal Queries (Diff Between States)

Compare two version vectors structurally:

```bash
# What changed between two versions?
kerai diff --from '{"billy": 100}' --to '{"billy": 147, "agent-1": 83}'

# Output:
# src/auth/handler.rs:
#   + ItemFn parse_header (added by billy @ seq 112)
#   ~ ItemFn validate_token (modified by agent-1 @ seq 47)
#     ~ ExprIf condition changed
#     + ExprReturn added
#   - ItemFn old_helper (deleted by billy @ seq 130)
```

### 7.5 Postgres Full-Text Search Integration

Use Postgres's built-in full-text search for content-aware queries:

```sql
-- Search across all identifiers and literals
CREATE INDEX idx_nodes_content_fts ON nodes USING gin(to_tsvector('english', content));

-- Find nodes mentioning "token" or "auth"
SELECT path, content FROM nodes
WHERE to_tsvector('english', content) @@ to_tsquery('token | auth');
```

Exposed via CLI:
```bash
kerai search "token auth"           # full-text search across node content
kerai search --kind Comment "TODO"  # search only in comments
```

## Decisions to Make

- **Query permissions:** In shared-server mode, should all users have full SQL access? Proposed: yes, for now. Kerai databases are development tools, not production systems. Access control is at the connection level.
- **Performance guardrails:** Should `kerai query` have a timeout or row limit to prevent runaway queries? Proposed: 30-second default timeout, overridable with `--timeout`.
- **Graph traversal depth:** `callers` and `callees` are direct references. Should there be a `--depth N` flag for transitive closure? Proposed: yes, up to a configurable max depth.

## Out of Scope

- AI-weighted queries (Plan 08 adds the `perspectives` layer)
- Natural language query interface ("show me functions that handle errors") — this is a future plan that builds on Plan 08
- Visualization (graph rendering, dependency diagrams) — would be a separate tool
