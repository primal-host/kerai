# Plan 22 — CSV Import Extension

## Context

Kerai's parser family (Rust, Go, C, Markdown, LaTeX) creates nodes FROM content. The CSV parser introduces a fundamentally new pattern: it creates real Postgres tables AND structural nodes that point to them.

The immediate use case is importing Kaggle competition data (35 CSV files, ~7.2M rows) into typed Postgres tables, with kerai nodes representing the structural knowledge (schema, columns, cross-table relationships).

**Design principle**: Multi-pass import preserves raw data fidelity. All values are first stored as TEXT. Type promotion happens in a second pass. Columns where all values cast cleanly get promoted and the text column is dropped. Columns with casting failures keep both: a typed column (with NULLs for failures) and a `_raw` TEXT column (complete original data). The presence of `_raw` IS the signal that something didn't cast cleanly.

## Multi-Pass Architecture

### Pass 0 — Registry

Persistent infrastructure tables in the `kerai` schema:

```sql
CREATE TABLE kerai.csv_projects (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL UNIQUE,
    schema_name TEXT NOT NULL,
    source_dir  TEXT,
    created_at  TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE kerai.csv_files (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id  UUID NOT NULL REFERENCES kerai.csv_projects(id),
    filename    TEXT NOT NULL,
    table_name  TEXT NOT NULL,
    headers     TEXT[] NOT NULL,
    row_count   INTEGER,
    created_at  TIMESTAMPTZ DEFAULT now(),
    UNIQUE (project_id, filename)
);
```

### Pass 1 — Raw Ingest

For each CSV file:
1. Read headers, register in `kerai.csv_files`
2. Sanitize column names to snake_case; detect duplicates and suffix with `_2`, `_3` etc.
3. Create table `{schema}.{table}` with ALL columns as `TEXT`
4. Batch INSERT all data (500 rows/batch) — zero interpretation, zero data loss
5. Update `row_count` in `kerai.csv_files`

Table naming: `MTeams.csv` → `m_teams`, `MNCAATourneyCompactResults.csv` → `mncaa_tourney_compact_results`

### Pass 2 — Type Promotion

For each column in each table, try parsers in order: integer (BIGINT) → float (DOUBLE PRECISION) → date (DATE) → text (keep as-is).

**Clean promotion** (100% of non-empty values cast):
- Rename original TEXT column to `_tmp_text`
- Add typed column with original name
- Populate via CAST
- Drop `_tmp_text`

**Partial promotion** (some values fail to cast):
- Rename original TEXT column to `{name}_raw`
- Add typed column with original name
- Populate with CASE expression (cast successes → typed, failures → NULL)
- Both columns remain: `{name}` (typed) and `{name}_raw` (complete TEXT)

Type detection uses regex patterns:
- Integer: `^-?[0-9]+$`
- Float: `^-?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?$` (must contain `.` or `e`)
- Date: `^[0-9]{1,2}/[0-9]{1,2}/[0-9]{4}$` (MM/DD/YYYY) or `^[0-9]{4}-[0-9]{2}-[0-9]{2}$` (YYYY-MM-DD)

### Pass 3 — Kerai Nodes + Edges

1. Create `csv_dataset` node (root) for the project
2. Create `csv_table` nodes (one per file) with metadata pointing to the real table
3. Create `csv_column` nodes with type/stats metadata including `has_raw: true/false`
4. Detect shared column names across tables, create `shared_column` edges between all pairs

## Implementation

### New Files

| File | Role |
|------|------|
| `postgres/src/parser/csv/mod.rs` | pg_extern entry points: `parse_csv_file`, `parse_csv_dir` |
| `postgres/src/parser/csv/kinds.rs` | Node kind constants: `csv_dataset`, `csv_table`, `csv_column` |
| `postgres/src/parser/csv/registry.rs` | Pass 0: ensure tables, register project/file, update row_count |
| `postgres/src/parser/csv/ingest.rs` | Pass 1: derive_table_name, sanitize columns, create/load TEXT tables |
| `postgres/src/parser/csv/promote.rs` | Pass 2: analyze and promote columns with ColumnStats tracking |
| `postgres/src/parser/csv/nodes.rs` | Pass 3: create dataset/table/column nodes + shared_column edges |

### Modified Files

| File | Change |
|------|--------|
| `postgres/src/parser/mod.rs` | `pub mod csv;` |
| `postgres/src/parser/kinds.rs` | `CsvDataset`, `CsvTable`, `CsvColumn` in Kind enum + as_str/from_str/ALL |
| `postgres/Cargo.toml` | `csv = "1"` dependency |
| `postgres/src/schema.rs` | Registry DDL, `parse_csv` reward (10 Koi) |
| `kerai/src/main.rs` | `ImportCsv` in PostgresAction with `path`, `--schema`, `--project` args |
| `kerai/src/commands/mod.rs` | `ImportCsv` in Command enum + dispatch |
| `kerai/src/commands/import.rs` | `run_csv()` — detects file vs dir, calls pg_extern functions |

### Node/Edge Structure

**Dataset node** (`csv_dataset`):
```json
{"schema": "kaggle", "project": "march-machine-learning-mania-2026",
 "source_dir": "/path/to/dir", "table_count": 35, "total_rows": 7213256}
```

**Table node** (`csv_table`):
```json
{"schema": "kaggle", "table_name": "m_teams", "source_file": "MTeams.csv",
 "row_count": 381, "column_count": 4, "nil_total": 0,
 "qualified_name": "kaggle.m_teams", "project_id": "uuid"}
```

**Column node** (`csv_column`):
```json
{"data_type": "BIGINT", "original_name": "TeamID", "position": 0,
 "has_raw": false, "nil_count": 0, "nil_rate": 0.0, "empty_count": 0,
 "unique_count": 381, "min": "1101", "max": "1481"}
```

**Shared column edge** (`shared_column`):
```json
{"column_name": "season", "source_table": "m_teams",
 "target_table": "m_regular_season_compact_results"}
```

### CLI Usage

```bash
# Import a competition directory
kerai postgres import-csv /path/to/dir --schema kaggle --project march-machine-learning-mania-2026

# Import a single file
kerai postgres import-csv /path/to/file.csv --schema kaggle --project my-project
```

### SQL Usage

```sql
-- Import directory
SELECT kerai.parse_csv_dir('/path/to/dir', 'kaggle', 'march-machine-learning-mania-2026');

-- Import single file
SELECT kerai.parse_csv_file('/path/to/file.csv', 'kaggle', 'my-project');
```

## Verification Results

Tested with March Machine Learning Mania 2026 dataset (35 CSV files):

| Metric | Value |
|--------|-------|
| Tables created | 35 |
| Total rows | 7,213,256 |
| Largest table | `m_massey_ordinals` (5,761,702 rows) |
| Kerai nodes | 316 (1 dataset + 35 tables + 280 columns) |
| Shared column edges | 1,035 |
| `_raw` columns | 0 (all data was clean) |
| Import time | ~430 seconds |

Type promotion verified:
- `TeamID` → BIGINT, `TeamName` → TEXT (m_teams)
- `DayZero` → DATE (m_seasons, MM/DD/YYYY format)
- `Season`, `DayNum`, scores → BIGINT
- `WLoc`, region names, conference abbreviations → TEXT

## Key Reuse

- `parser::inserter` — batch node/edge INSERT (500/batch)
- `parser::ast_walker` — NodeRow/EdgeRow structs
- `parser::path_builder` — ltree path construction
- `sql` module — SQL escaping helpers
- Reward minting via `kerai.mint_reward()`
