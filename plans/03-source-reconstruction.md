# Plan 03: Source Reconstruction

*Depends on: Plan 02 (Rust Parser)*
*Enables: Plan 05 (CLI)*

## Goal

Reconstruct valid, `rustfmt`-compliant Rust source files from the database. This is the inverse of Plan 02 — the round-trip must be lossless. Parse a Rust file, store it in the database, reconstruct it, and `rustfmt` of the original and `rustfmt` of the reconstruction must be byte-identical.

## Why This Matters

The database is the source of truth, but developers edit text. The workflow is:

1. Reconstruct source files from the database → developer edits them → re-parse into the database
2. An agent modifies nodes directly in the database → reconstruct source to verify/test/compile

If reconstruction isn't faithful, developers can't trust the system.

## Deliverables

### 3.1 AST Reconstruction

A Rust module (part of the pgrx extension) that:

1. Queries the database for all nodes under a given file node
2. Rebuilds a `syn::File` from the node tree:
   - Walk `nodes` by `parent_id` / `position` ordering
   - Map `kind` back to the corresponding `syn` type
   - Restore `content` into identifiers, literals, operators
   - Rebuild attributes from `Attribute` nodes
   - Reattach doc comments to their associated items
3. Uses `prettyplease::unparse()` to render the `syn::File` to source text
4. Optionally pipes through `rustfmt` for final canonical formatting

### 3.2 The `rustfmt` Advantage

Rust's canonical formatter means we don't need to store most whitespace, indentation, or style choices. `rustfmt` deterministically produces the same output for the same AST (given the same configuration).

**The round-trip guarantee:** `rustfmt(original) == rustfmt(reconstruct(parse(original)))`

We do NOT guarantee that the raw original is preserved — only the `rustfmt`'d version. This is acceptable because `rustfmt` ships with `rustup` and is the community standard.

**`prettyplease` vs `rustfmt`:** `prettyplease` is a pure-Rust formatter that operates directly on `syn` trees — no external process needed. It produces clean, idiomatic output that is close to but not identical to `rustfmt`. For exact `rustfmt` output, we pipe `prettyplease` output through `rustfmt` as a post-processing step. The extension uses `prettyplease` internally for speed; the CLI applies `rustfmt` for final output when exactness matters.

### 3.3 Crate and Module Reconstruction

Beyond individual files:

- Reconstruct `Cargo.toml` from the crate metadata nodes
- Reconstruct module hierarchy (inline modules vs file modules vs `mod.rs` patterns)
- Place files in the correct directories based on module paths
- Write a full buildable crate to a target directory

### 3.4 Selective Reconstruction

Not always reconstruct everything:

- Reconstruct a single file: `kerai checkout --file src/auth/handler.rs`
- Reconstruct a module: `kerai checkout --module auth`
- Reconstruct the full crate: `kerai checkout`

These map to tree queries: get the file node and its subtree, get the module node and its subtree, get the crate root and everything.

### 3.5 Extension Integration

Reconstruction is available as SQL-callable functions:

```sql
-- Reconstruct a file to source text
SELECT kerai.reconstruct_file(file_node_id);

-- Reconstruct a module
SELECT kerai.reconstruct_module('auth');

-- Reconstruct the full crate to a directory
SELECT kerai.reconstruct_crate('/tmp/output');
```

### 3.6 Round-Trip Test Suite

An automated test that:

1. Takes a corpus of real Rust crates (popular crates like `serde`, `tokio`, `clap`, plus kerai's own source)
2. Parses them into the database (Plan 02)
3. Reconstructs them from the database
4. Compares `rustfmt(original)` vs `rustfmt(reconstructed)` byte-for-byte
5. Fails if any file differs

This is the critical correctness gate. If the round-trip breaks, nothing else works.

**Macro round-trip caveat:** Macro invocations round-trip (the `macro_name!(...)` call is preserved). Macro expansions do not — they're derived and recomputed by the compiler. This is correct behavior: we store source, not compiler output.

## Decisions to Make

- **Comment placement heuristics:** Regular comments (`//`) are not in `syn`'s AST. We extract them from source text during parsing and associate them via an edge (`relation = 'documents'`). During reconstruction, we re-insert them at the correct positions. This is the hardest reconstruction problem.
- **Blank lines:** `rustfmt` has opinions about blank lines but preserves some developer choices. We may need to store blank-line counts in metadata to get byte-identical round-trips.
- **`cfg` variants:** Files conditionally compiled via `#[cfg]` are parsed in Plan 02 with their conditions stored. Reconstruction emits them with the conditions preserved — the conditional compilation is the compiler's problem, not ours.
- **Formatter configuration:** `rustfmt` respects `rustfmt.toml`. We store the formatter config as crate metadata and use it during reconstruction.

## Out of Scope

- Editing source and re-parsing (that's the CLI workflow in Plan 05)
- Reconstructing non-Rust source (future plans)
- Pretty-printing the AST in a non-Rust format (e.g., a tree visualization)
