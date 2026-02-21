# Plan 23 — Stack Machine with Typed Pointers

## Context

Kerai's language module currently parses three notation modes (prefix, infix, postfix) into an `Expr` tree and evaluates it as a calculator. The precursor language tau proposed a stack-based system where the stack holds integer IDs pointing to typed server-side objects, with a web-first Jupyter-like UI.

Kerai already has Postgres as its data store and a growing set of parsers that create both real tables (CSV import) and structural knowledge nodes. The natural evolution: **the stack holds typed pointers, and every pointer refers to something in Postgres**. The workspace stack itself is always persisted to Postgres — push `[1 2 3]` without consuming it, come back tomorrow, it's still there.

**Design principle**: Postgres is the universal backbone. All data — tables, nodes, session state, stack contents — lives in Postgres. Import brings data in, export serializes it out. The stack is a persistent workspace, not ephemeral memory.

**Design principle**: Postgres all the way down. The interpreter state *is* the database. The stack, word definitions, dispatch table, object store — everything starts as Postgres rows and SQL operations. If a specific operation proves too slow (tight arithmetic loops, hot stack manipulation), promote just that piece to Rust. The database is the default; Rust is the escape hatch. Since pgrx runs Rust inside the Postgres process, a word like `+` can start as SQL and become a Rust function without changing its interface to the rest of the machine. Object IDs use Postgres sequences — stable, never renumbered, gaps are permanent. A pointer to object 42 is always object 42.

## Stack Pointer Structure

```rust
struct Ptr {
    kind: &'static str,    // "csv_project", "csv_table", "query_result", "text", "int", "list", ...
    ref_id: String,         // UUID, qualified table name, or literal value — depends on kind
    meta: serde_json::Value, // kind-specific metadata (row counts, column info, etc.)
}
```

The stack is `Vec<Ptr>`, persisted to Postgres as JSONB in the session table. Every mutation to the stack writes through to the session row.

**Inline vs. referenced**: Primitives (int, float, string, list) carry their value directly in `ref_id`/`meta`. Large/persistent data (tables, projects, query results, nodes) store a UUID or qualified name in `ref_id` that points to a Postgres table or row. The `kind` field tells the machine which is which. Either way, the pointer itself is always persisted in the session's stack column — nothing is lost on disconnect.

## How It Works — The csv.import Example

```
kerai> "/path/to/march-machine-learning-mania-2026" csv.import
```

Step by step:

1. **`"/path/to/march-machine-learning-mania-2026"`** — pushes `Ptr { kind: "path", ref_id: "/path/to/...", meta: {} }`.

2. **`csv.import`** — pops the path pointer, inspects kind:
   - `"path"` + directory → runs `parse_csv_dir()`, derives project name from basename
   - `"path"` + `.zip` → extracts to temp dir, runs `parse_csv_dir()` on contents
   - `"path"` + `.csv` → runs `parse_csv_file()`
   - Pushes result: `Ptr { kind: "csv_project", ref_id: "<project-uuid>", meta: { "schema": "kaggle", "tables": 35, "rows": 7213256 } }`

3. **`edit`** — pops the top pointer, dispatches on `kind`:
   - `"csv_project"` → opens project explorer in web UI (table list, row counts, column types)
   - `"csv_table"` → opens data grid editor
   - `"text"` → opens text editor
   - `"query_result"` → opens result table view

Dispatch is fully late-bound. `edit` doesn't know about CSV — it looks up a handler registry keyed by `kind`.

## Stack Persistence

```sql
CREATE TABLE kerai.sessions (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    TEXT NOT NULL,
    stack      JSONB NOT NULL DEFAULT '[]',  -- serialized Vec<Ptr>
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);
```

Every stack mutation writes through to this row. Push `[1 2 3]` and walk away — reconnect later and it's still on top of the stack. Since the stack only holds pointers (not bulk data), serialization is trivial. The actual data behind referenced pointers is already in Postgres tables — it doesn't go anywhere.

## Postgres as Universal Backbone

Every piece of data lives in Postgres:

| What | Where |
|------|-------|
| Raw CSV data | `kaggle.m_teams`, etc. (real tables) |
| Project metadata | `kerai.csv_projects`, `kerai.csv_files` |
| Knowledge graph | `kerai.nodes`, `kerai.edges` |
| Session/stack state | `kerai.sessions` |
| User definitions | `kerai.definitions` (new) |
| Named objects | `kerai.objects` (new, generic typed store) |

No external file storage, no in-memory-only state. Crash → restart → reload session → all data still there.

## Import / Export Symmetry

```
kerai> "/data/competition.zip" csv.import       # zip → project in Postgres
kerai> csv.export                                # top of stack (csv_project) → zip file
kerai> "kaggle.m_teams" csv.export               # single table → CSV file
kerai> project.export                            # project → zip with CSVs + node metadata
```

`csv.export` dispatches on what's on the stack:
- `csv_project` → exports all tables as CSVs in a zip
- `csv_table` → exports one table as a CSV
- `query_result` → exports query results as a CSV

`project.export` is broader — includes kerai node/edge metadata alongside raw data, so someone can `project.import` it on their kerai instance and get the full knowledge graph.

## Type Dispatch Table

```rust
type Handler = fn(stack: &mut Vec<Ptr>, db: &PgPool) -> Result<(), Error>;

struct Machine {
    stack: Vec<Ptr>,
    db: PgPool,
    session_id: Uuid,
    handlers: HashMap<String, Handler>,           // "csv.import" → handler_csv_import
    type_methods: HashMap<(String, String), Handler>, // ("csv_project", "edit") → handler_edit_project
}
```

Word resolution order:
1. Check `handlers` for exact match (e.g., `csv.import` always does the same thing)
2. Check `type_methods` keyed by `(top_of_stack.kind, word)` (e.g., `("csv_project", "edit")`)
3. Try parsing as literal (int, float, string, list) and push
4. Error: unknown word

This gives both explicit commands and type-dispatched methods.

## Libraries as Namespaces

The current CLI's top-level command groups (`postgres`, `wallet`, `swarm`, `model`, `csv`, etc.) become **libraries** — namespaces of words. Each library name is itself a word that pushes a library pointer onto the stack.

### Dot Form vs. Space Form

Both forms resolve to the same handler:

```
postgres.ping          # dot form — direct call, single token
postgres ping          # space form — push library, then dispatch "ping" on it
```

The dot form is compact for scripts. The space form reads naturally on the command line and in conversation.

### Argument Placement Rule

**Arguments go before the word that consumes them.** This is the fundamental postfix principle applied consistently:

- **Library configuration** (connection strings, hosts) goes before the library word
- **Action arguments** (patterns, table names, counts) go before the action word

```
"123.12.1.0" postgres ping                  # address → postgres, ping takes nothing
"123.12.1.0" postgres "%parse%" find        # address → postgres, pattern → find
postgres "%parse%" find                      # default postgres, pattern → find
postgres ping                                # default postgres, ping takes nothing
"prod-db" postgres "m_teams" 10 rows        # connect to prod, get 10 rows from m_teams
```

Each word consumes exactly what belongs to it. Library words consume configuration. Action words consume action arguments. Reads left-to-right as "configure, then act."

### Library Pointer Dispatch

When the machine encounters a library name like `postgres`:

1. Check the stack top — if there's an address/connection string, pop it and bind
2. Push `Ptr { kind: "library", ref_id: "postgres", meta: { "host": "..." } }`

When the next word (e.g., `ping`) executes:

1. See `kind: "library"` on stack top → look up `("library:postgres", "ping")` in type_methods
2. Resolves to the same handler as the dot form `postgres.ping`

### Aliases

Libraries can be aliased like any definition:

```
pg: postgres
pg ping                    # same as postgres ping
pg.ping                    # same as postgres.ping
```

### Shell Invocation

Everything after `kerai` on the command line is input to the postfix interpreter:

```bash
kerai postgres ping                         # evaluate "postgres ping"
kerai "123.12.1.0" postgres ping            # evaluate with remote host
kerai "/path/to/dir" csv import             # evaluate csv import
kerai 1 2 +                                 # evaluate arithmetic
kerai                                       # no args → REPL
```

The shell's word splitting tokenizes for free. Quoted strings pass through as single tokens. This **replaces the clap subcommand tree entirely** — the kerai binary joins `argv[1..]` into a postfix expression, evaluates it, prints the stack, and exits.

### Current CLI Groups → Libraries

| Current CLI | Library | Example Words |
|------------|---------|---------------|
| `kerai postgres <action>` | `postgres` | `ping`, `find`, `query`, `tree`, `refs`, `import`, `export` |
| `kerai postgres import-csv` | `csv` | `import`, `export` |
| `kerai wallet <action>` | `wallet` | `create`, `balance`, `transfer`, `history` |
| `kerai currency <action>` | `currency` | `register`, `transfer`, `supply`, `schedule` |
| `kerai market <action>` | `market` | `create`, `bid`, `settle`, `browse`, `stats` |
| `kerai bounty <action>` | `bounty` | `create`, `list`, `show`, `claim`, `settle` |
| `kerai swarm <action>` | `swarm` | `launch`, `status`, `stop`, `leaderboard` |
| `kerai model <action>` | `model` | `create`, `train`, `predict`, `search`, `info` |
| `kerai agent <action>` | `agent` | `add`, `list`, `remove`, `info` |
| `kerai peer <action>` | `peer` | `add`, `list`, `remove`, `info` |
| `kerai config <action>` | `config` | `get`, `set`, `list`, `delete` |
| `kerai alias <action>` | `alias` | `get`, `set`, `list`, `delete` |

Generic words that dispatch on stack type rather than belonging to a namespace: `edit`, `export`, `import`, `info`, `list`, `show`, `delete`.

## Composable Command Chains

```
kerai> "data.zip" csv.import                          # push csv_project
kerai> "m_teams" table.select                         # pop project, push csv_table for m_teams
kerai> [season teamid] columns.pick                   # pop table, push filtered table view
kerai> edit                                           # open grid editor for the filtered view
kerai> "SELECT * FROM kaggle.m_teams WHERE teamid > 1400" sql  # push query_result
kerai> dup csv.export                                 # export without consuming the result
```

Stack manipulation words: `dup`, `swap`, `drop`, `over`, `rot`, `clear` — standard Forth vocabulary.

## Web UI Integration

The existing Axum server adds:

- **`POST /api/eval`** — sends input text, receives stack as JSON array of `Ptr` objects
- **`GET /api/object/:kind/:ref_id`** — fetches renderable content for a pointer
- **WebSocket `/api/ws`** — live updates (import progress, stack changes)

The browser renders each stack item as a card. Card appearance is determined by `kind`:
- `csv_project` → table list with row counts
- `csv_table` → data preview (first 20 rows)
- `query_result` → full result grid
- `int`/`float`/`text`/`list` → inline display
- `error` → red card

`edit` tells the browser to open an interactive editor for the top card.

## Migration Path from Current Lang Module

1. **Keep** `token.rs` — already handles whitespace-separated words, quoted strings, brackets
2. **Keep** `Expr` tree and Pratt parser — useful for infix arithmetic subexpressions
3. **Replace** `eval.rs` `Value` enum with `Ptr` — stack type changes from `Value` to `Ptr`
4. **Add** `machine.rs` — dispatch loop that processes postfix word streams
5. **Add** handler modules — `handlers/csv.rs`, `handlers/stack.rs` (dup/swap/drop), `handlers/query.rs` (SQL), `handlers/io.rs` (import/export)
6. **Wire** into web server — `/api/eval` endpoint runs the machine

The existing `Value::Int/Float/Str/List` maps directly to pointer kinds:
- `Value::Int(42)` → `Ptr { kind: "int", ref_id: "42", meta: null }`
- `Value::List([1,2,3])` → `Ptr { kind: "list", ref_id: "", meta: [1,2,3] }`

Arithmetic operations pop ptr values, compute, push result. The difference is that *some* pointers refer to large Postgres-resident objects, and the machine knows how to operate on both.

## Evolution from Tau

| Aspect | Tau (original) | Kerai (proposed) |
|--------|---------------|-----------------|
| Stack values | Integer IDs into a Go map | Typed `Ptr` structs with kind/ref/meta |
| Object store | In-memory `map[int64]*Object` | Postgres (tables, nodes, or inline in stack) |
| Language | Go, standalone binary | Rust, Postgres extension + CLI + web |
| GC | Unspecified | Postgres handles it (DROP TABLE, session cleanup) |
| Persistence | `~/.tau` files | Postgres entirely — stack survives disconnects |
| Type dispatch | Unspecified | `(kind, word) → handler` lookup table |
| File format | `.tau` with comment blocks | `.kerai` with notation directives |

## Implementation

### New Files

| File | Role |
|------|------|
| `kerai/src/lang/ptr.rs` | `Ptr` struct, serialization, inline value helpers |
| `kerai/src/lang/machine.rs` | `Machine` struct, dispatch loop, stack persistence |
| `kerai/src/lang/handlers/mod.rs` | Handler trait, registry |
| `kerai/src/lang/handlers/stack.rs` | dup, swap, drop, over, rot, clear |
| `kerai/src/lang/handlers/csv.rs` | csv.import, csv.export |
| `kerai/src/lang/handlers/query.rs` | sql, table.select, columns.pick |
| `kerai/src/lang/handlers/io.rs` | project.export, project.import |
| `kerai/src/serve/routes/eval.rs` | POST /api/eval endpoint |
| `kerai/src/serve/routes/object.rs` | GET /api/object/:kind/:ref_id endpoint |

### Modified Files

| File | Change |
|------|--------|
| `kerai/src/lang/mod.rs` | Add `pub mod ptr;`, `pub mod machine;`, `pub mod handlers;` |
| `kerai/src/lang/eval.rs` | Adapt Value → Ptr for arithmetic ops |
| `kerai/src/serve/mod.rs` | Wire eval/object routes |
| `postgres/src/schema.rs` | Add `kerai.sessions` DDL |

### Schema Additions

```sql
CREATE TABLE kerai.sessions (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    TEXT NOT NULL,
    stack      JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE kerai.definitions (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID REFERENCES kerai.sessions(id),
    name       TEXT NOT NULL,
    body       TEXT NOT NULL,           -- stored as kerai source text
    created_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE (session_id, name)
);
```

## What This Gets You

1. **Persistent workspace**: push values, disconnect, reconnect — stack is intact
2. **Shareable**: `project.export` gives someone a zip; `project.import` restores it
3. **Queryable**: everything is SQL — `SELECT * FROM kaggle.m_teams WHERE teamid > 1400`
4. **Composable**: stack operations chain — `"data.zip" csv.import "m_teams" table.select [season teamid] columns.pick`
5. **Extensible**: new kinds just need a handler — `go.import`, `rust.import`, `pdf.import` all follow the same pattern
6. **Recoverable**: crash → restart → reload session → all data still there
