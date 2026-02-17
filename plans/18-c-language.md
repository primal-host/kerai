# Plan 18: C Language Support via tree-sitter

*Depends on: Plan 02 (Rust Parser), Plan 15 (Comment Handling), Plan 17 (Go / tree-sitter infra)*

## Summary

Add C as the third supported language, reusing the tree-sitter pipeline established by Go. C stress-tests the multi-language architecture with challenges unique to the language: declarator chains (pointer → function → array nesting), preprocessor directives as first-class AST nodes, no built-in module/package system, and the `static` keyword for file-scoped visibility.

The implementation replicates the Go pattern: `CWalkCtx` accumulator, `c_`-prefixed kind constants, `parse_c_source()`/`parse_c_file()` entry points, reconstruction from `metadata.source`, and shared `match_comments_to_ast`. The only structural addition is `unwrap_declarator_name()` — a recursive helper to extract identifiers from C's nested declarator chains.

## Architecture

### C Parser (`src/parser/c/`)

- `kinds.rs` — ~65 string constants prefixed with `c_` mapped from tree-sitter-c grammar node types
- `walker.rs` — CST walker using `CWalkCtx` accumulator with `unwrap_declarator_name()` for C's nested declarators
- `metadata.rs` — C-specific metadata extraction (return_type, static, params, storage_class, system includes)
- `mod.rs` — `parse_c_source()` and `parse_c_file()` `#[pg_extern]` entry points
- `suggestion_rules.rs` — Five initial rules for C idioms and code quality

### C Reconstruction (`src/reconstruct/c.rs`)

- `reconstruct_c_file()` — validates language='c', emits source from metadata

### Modified Files

- `Cargo.toml` — Added `tree-sitter-c = "0.23"`
- `src/parser/treesitter/mod.rs` — Added `TsLanguage::C` variant
- `src/parser/mod.rs` — Added `pub mod c;`
- `src/reconstruct/mod.rs` — Added `mod c;`

## C Kind Constants

All C kinds are prefixed with `c_` to avoid collisions with Rust and Go kinds. Key mappings:

| tree-sitter node type | kerai kind |
|---|---|
| `function_definition` | `c_function` |
| `declaration` | `c_declaration` |
| `type_definition` | `c_typedef` |
| `struct_specifier` | `c_struct` |
| `union_specifier` | `c_union` |
| `enum_specifier` | `c_enum` |
| `field_declaration` | `c_field` |
| `enumerator` | `c_enumerator` |
| `preproc_include` | `c_include` |
| `preproc_def` | `c_define` |
| `preproc_function_def` | `c_macro` |
| `preproc_ifdef` | `c_ifdef` |
| `pointer_declarator` | `c_pointer_decl` |
| `compound_statement` | `c_block` |

## C-Specific Design

### Declarator Chain Unwrapping

C's type system nests declarators arbitrarily deep. `int *(*fp)(int, char)` (pointer to function returning `int*`) produces:

```
pointer_declarator → parenthesized_declarator → pointer_declarator → function_declarator → identifier
```

The `unwrap_declarator_name()` helper recursively walks inward through any combination of `pointer_declarator`, `array_declarator`, `function_declarator`, `parenthesized_declarator`, and `attributed_declarator` to find the `identifier` node.

### Preprocessor Directives

Unlike Go and Rust, C's preprocessor directives appear as first-class named nodes in the tree-sitter AST. They are walked and stored like declarations:

- `#include` → `c_include` with `path` and `system` (true for `<>`, false for `""`) metadata
- `#define` → `c_define` with `name` and `value` metadata
- `#define NAME(args) body` → `c_macro` with `name`, `parameters`, and `value` metadata
- `#ifdef`/`#if` → container nodes with conditional children

### Static Storage Class

Instead of Go's `is_exported()` (uppercase = public), C uses `static` for file-scoped (internal) linkage. The walker checks for `storage_class_specifier` children with value `"static"` and stores `"static": true` in function/declaration metadata.

### Forward Declarations and Anonymous Types

- `struct Foo;` (no body) still produces a `c_struct` node with name but no children
- `typedef struct { int x; } Point;` — the anonymous struct gets a `c_struct` child under the `c_typedef` node; only the typedef carries the name

## Suggestion Rules

| Rule ID | Severity | Category | Description |
|---------|----------|----------|-------------|
| `c_no_void_param` | info | idiom | `func()` should be `func(void)` for explicit zero-arg |
| `c_global_no_static` | info | visibility | Non-static file-scope variable — consider `static` |
| `c_magic_number` | info | readability | Placeholder for numeric literal detection |
| `c_long_function` | warning | complexity | Function body exceeds 50 lines |
| `c_missing_include_guard` | info | idiom | Header file without `#ifndef`/`#define` guard |

## Tests

11 `#[pg_test]` integration tests:

1. `test_parse_c_source_basic` — Minimal C file produces nodes
2. `test_c_function_node_kind` — `int main(void)` → `c_function` kind
3. `test_c_static_metadata` — `static` function has `static: true`
4. `test_c_struct_fields` — Struct with 3 fields → 3 `c_field` nodes
5. `test_c_enum_enumerators` — Enum with 3 values → 3 `c_enumerator` nodes
6. `test_c_include_metadata` — `<stdio.h>` has `system: true`, `"foo.h"` has `system: false`
7. `test_c_define_metadata` — `#define MAX_SIZE 100` has correct name and value
8. `test_c_comment_documents_edge` — Comment above function → `documents` edge
9. `test_c_pointer_function` — `int *foo(int x)` unwraps declarator to name `foo`
10. `test_c_reconstruct_roundtrip` — Parse → reconstruct → verify key elements present
11. `test_c_typedef` — `typedef struct { ... } Point;` → `c_typedef` node named `Point`

## Cross-Language Queries

With Rust, Go, and C all parsed, cross-language queries work naturally:

```sql
-- All functions across three languages
SELECT content, language, kind FROM kerai.nodes
WHERE kind IN ('fn', 'go_func', 'c_function');

-- All struct definitions
SELECT content, language, kind FROM kerai.nodes
WHERE kind IN ('struct', 'go_struct', 'c_struct');

-- C #include dependencies
SELECT metadata->>'path', metadata->>'system'
FROM kerai.nodes WHERE kind = 'c_include';
```

## Status

**Implemented.** All steps complete with 11 integration tests passing (254 total).
