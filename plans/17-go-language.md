# Plan 17: Go Language Support via tree-sitter

*Depends on: Plan 02 (Rust Parser), Plan 15 (Comment Handling)*

## Summary

Add Go as the second supported language using tree-sitter for parsing instead of a Go-specific crate. This establishes the multi-language foundation: adding Python, TypeScript, C later requires only a grammar crate and a walker module.

The database schema is already language-agnostic (`kind TEXT`, `language TEXT`, `metadata JSONB`). The inserter, path builder, comment extractor, and normalizer are all reused without modification.

## Architecture

### tree-sitter Infrastructure (`src/parser/treesitter/`)

- `TsLanguage` enum — extensible to new languages via additional variants
- `parse()` — language-dispatching parser wrapper
- `cursor.rs` — generic helpers: `node_text()`, `span_start_line()`, `span_end_line()`

### Go Parser (`src/parser/go/`)

- `kinds.rs` — ~80 string constants prefixed with `go_` mapped from tree-sitter grammar node types
- `walker.rs` — CST walker using `GoWalkCtx` accumulator pattern (mirrors Rust's `WalkCtx`)
- `metadata.rs` — Go-specific metadata extraction (exported, receiver, params, tags)
- `mod.rs` — `parse_go_source()` and `parse_go_file()` `#[pg_extern]` entry points
- `suggestion_rules.rs` — Three initial rules: `go_exported_no_doc`, `go_error_not_last`, `go_stutter`

### Go Reconstruction (`src/reconstruct/go.rs`)

- `reconstruct_go_file()` — validates language='go', emits source from metadata

## Go Kind Constants

All Go kinds are prefixed with `go_` to avoid collisions with Rust kinds. Key mappings:

| tree-sitter node type | kerai kind |
|---|---|
| `function_declaration` | `go_func` |
| `method_declaration` | `go_method` |
| `type_spec` | `go_type_spec` |
| `struct_type` | `go_struct` |
| `interface_type` | `go_interface` |
| `field_declaration` | `go_field` |
| `import_spec` | `go_import_spec` |

## Suggestion Rules

| Rule ID | Severity | Description |
|---------|----------|-------------|
| `go_exported_no_doc` | info | Exported symbol has no doc comment |
| `go_error_not_last` | warning | Function returns error in non-last position |
| `go_stutter` | info | Type name stutters with package name |

## Status

**Implemented.** All steps complete with 9 integration tests passing.
