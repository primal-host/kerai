# Plan 02: Rust Parser

*Depends on: Plan 01 (Foundation)*
*Enables: Plan 03 (Source Reconstruction), Plan 05 (CLI)*

## Goal

Parse Rust source files into the `nodes` and `edges` tables. At the end of this plan, you can point kerai at a Rust crate and it populates the database with the full AST, resolved identifiers, and cross-reference edges. Since kerai itself is a Rust pgrx extension, this is dogfooding — kerai parses its own codebase.

## Why Rust First

- **We're writing kerai in Rust.** The pgrx extension is Rust. The `syn` crate gives native AST access with zero FFI. We dogfood kerai on its own codebase from day one.
- **`syn` is production-quality.** It's the foundation of Rust's entire procedural macro ecosystem — battle-tested on every crate that uses `derive`. Full-fidelity parsing of all Rust syntax.
- **`rustfmt` provides canonical formatting.** Like Go's `gofmt`, Rust has a canonical formatter that ships with `rustup`. This eliminates the whitespace storage problem.
- **Cargo's dependency model is well-specified.** `Cargo.toml` and `Cargo.lock` define the dependency graph precisely. Content-addressed via lockfile hashes.
- **If we handle Rust, Go will be easier.** Rust's grammar (macros, lifetimes, generics, trait bounds, async/await, pattern matching) is significantly richer than Go's. Building the parser for the harder language first ensures the node/edge model is general enough.

## Deliverables

### 2.1 Syntax Tree Ingestion

A Rust module (part of the pgrx extension) that:

1. Takes a crate root path (directory containing `Cargo.toml`)
2. Uses `syn::parse_file()` to parse all `.rs` files in the crate
3. Walks the AST and inserts rows into `nodes`:
   - One node per AST node (crate, module, function, impl block, struct, enum, statement, expression, pattern, identifier, literal)
   - `parent_id` reflects the AST parent-child structure
   - `position` preserves sibling order
   - `path` (ltree) computed as the node is inserted
   - `kind` maps from `syn`'s type names: `ItemFn`, `ItemStruct`, `ItemEnum`, `ItemImpl`, `ExprIf`, `ExprCall`, `ExprMatch`, `Pat`, `Ident`, `Lit`, etc.
   - `content` populated for leaf nodes: identifier names, literal values, operators
   - `metadata` stores Rust-specific attributes: visibility (`pub`, `pub(crate)`), generics, lifetime parameters, `#[cfg]` conditions, `#[derive]` lists, `async`, `unsafe`, etc.

Callable as an extension function:

```sql
-- Parse a crate into the database
SELECT kerai.parse_crate('/path/to/crate');

-- Re-parse a single file
SELECT kerai.parse_file('/path/to/crate/src/lib.rs');

-- Parse from source text (for agents that generate code directly)
SELECT kerai.parse_source('fn hello() { println!("world"); }', 'snippet.rs');
```

### 2.2 Attribute and Comment Preservation

Rust's attributes and doc comments are syntactically significant:

- Store `#[...]` outer attributes and `#![...]` inner attributes as nodes with `kind = 'Attribute'`
- Store doc comments (`///`, `//!`) as `kind = 'DocComment'` — these are syntactic sugar for `#[doc = "..."]`
- Regular comments (`//`, `/* */`) are not in `syn`'s AST by default. Use `syn::parse_file()` with full span info plus a separate comment extraction pass from the source text.
- Associate comments with their nearest AST node via an edge (`relation = 'documents'`)
- Preserve `#[cfg(...)]` conditions in `metadata` — these affect which AST exists for which target

### 2.3 Type Resolution and Cross-References

Using rust-analyzer's published libraries (`ra_ap_*` crates):

1. Load the crate with full type information via `ra_ap_ide` and `ra_ap_hir`
2. For every identifier, resolve what it refers to:
   - Local variable → declaration site (let binding, function parameter, match arm)
   - Imported item → the declaring module's node
   - Method call → the impl block's function node
   - Trait method → the trait definition's function node
3. Insert `edges` rows:
   - `relation = 'declares'` — declaration site to identifier
   - `relation = 'references'` — use site to declaration
   - `relation = 'calls'` — call expression to the function being called
   - `relation = 'imports'` — use statement to the module/item
   - `relation = 'type_of'` — expression to its resolved type
   - `relation = 'implements'` — impl block to the trait it implements
   - `relation = 'derives'` — struct/enum to the trait derived via `#[derive]`
   - `relation = 'lifetime_of'` — lifetime parameter to the type it annotates

### 2.4 Crate Metadata

- Parse `Cargo.toml` and store crate name, version, edition, dependencies as nodes
- Parse `Cargo.lock` and store resolved dependency versions and hashes in metadata
- The crate root is the top-level node; modules are children; files are children of modules (or modules themselves for `mod.rs` / inline modules)

### 2.5 Granularity Decision

**Proposed:** Parse down to the expression level, not the token level.

- `ItemFn`, `ItemStruct`, `ItemEnum`, `ItemImpl`, `ExprIf`, `ExprMatch`, `ExprCall`, `ExprClosure` — all get nodes
- Individual tokens (parentheses, semicolons, `fn` keyword) do NOT get nodes
- Identifiers, literals, and patterns DO get nodes (they're the leaves)
- Lifetime annotations and generic parameters get nodes (they're structurally significant in Rust)
- This keeps the row count manageable while preserving enough structure for meaningful merges

A modest Rust crate (~50 files, ~10K lines) should produce roughly 80K-300K nodes at this granularity (higher than Go due to Rust's richer syntax). A large crate (~500 files) might produce 2-5M nodes. Both are within Postgres comfort zone.

### 2.6 Macro Handling

Rust macros are the hardest challenge. The strategy:

- **Declarative macros (`macro_rules!`):** Store the macro definition as a node. At call sites, store the unexpanded invocation as a node with `kind = 'MacroCall'`. Do NOT store the expansion — it's derived and can be recomputed.
- **Procedural macros (`#[derive(...)]`, attribute macros, function-like macros):** Same approach — store the invocation, not the expansion.
- **`metadata.macro_expanded = true`** flag on nodes that exist only in the expanded AST (for cases where we do record expansion for analysis purposes).
- For cross-references that go through macro expansion, the edge `metadata` records that resolution required macro expansion.

### 2.7 Incremental Re-parse

After the initial ingestion, support re-parsing a single file:

1. Parse the changed file with `syn::parse_file()`
2. Diff the new AST against the existing nodes for that file
3. Insert/update/delete only the changed nodes
4. Record the changes as operations (Plan 04)

This is the foundation for "commit" — changing a file produces a set of operations.

## Decisions to Make

- **Dependency depth:** Do we ingest the ASTs of dependencies (from `Cargo.lock`), or only the current crate? Proposed: current crate only for now. External crates are represented as stub nodes with enough info for edge resolution.
- **Test modules:** Treat `#[cfg(test)] mod tests` as part of the tree with `metadata.test = true`. Same for files in `tests/` and `benches/`.
- **Generated files:** Files under `target/` or with generated-code markers get `metadata.generated = true`.
- **Macro expansion depth:** Store unexpanded macros only (proposed), or optionally store one level of expansion? The unexpanded form is the source of truth, but expansion may be needed for full cross-reference resolution.
- **rust-analyzer version pinning:** The `ra_ap_*` crates follow rust-analyzer's release cadence and break API frequently. Pin to a specific version and update deliberately.

## Out of Scope

- Reconstructing source from the database (Plan 03)
- Non-Rust languages (Go support is a future plan — the node/edge model is language-agnostic)
- Build-target variants beyond the default (`#[cfg]` conditional compilation stores the conditions but doesn't parse multiple variants)
