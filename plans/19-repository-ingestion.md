# Plan 19: Repository Ingestion

*Depends on: Plan 02 (Rust Parser), Plan 15 (Comment Handling), Plan 17 (Go), Plan 18 (C)*

## Summary

Repository-level ingestion via git2: clone a repository, walk its commit graph, parse every file at HEAD through the appropriate language parser, and store unparsed files as opaque nodes. A single SQL call — `SELECT kerai.mirror_repo('https://github.com/user/repo')` — does everything. Re-running fetches new commits and only reprocesses changed files.

## New Dependencies

- `git2 = "0.19"` — libgit2 bindings for clone/fetch/diff
- `tempfile = "3"` — dev dependency for test repo creation

## Schema Changes

### `kerai.repositories` table
Tracks mirrored repositories with URL, local clone path, HEAD commit, and link to the root `repo_repository` node. Unique index on `(instance_id, url)`.

### Reward schedule
Added `('mirror_repo', 100)` to seed data.

## New Node Kinds

| Kind | Description |
|------|-------------|
| `repo_repository` | Root node for an ingested repo |
| `repo_commit` | Git commit with sha/author/message metadata |
| `repo_directory` | Directory in the file tree |
| `repo_tag` | Git tag reference |
| `repo_branch` | Git branch reference |
| `repo_opaque_text` | Unparsed text file (source in metadata, capped 100KB) |
| `repo_opaque_binary` | Binary file (sha256 + size in metadata) |

## Module Structure

```
src/repo/
├── mod.rs              # #[pg_extern] functions + orchestration
├── kinds.rs            # String constants for repo node kinds
├── cloner.rs           # git2 clone/fetch/head_sha
├── commit_walker.rs    # Commit graph → NodeRow/EdgeRow
├── tree_walker.rs      # File tree at HEAD → parser dispatch + opaque nodes
├── language_detect.rs  # Extension → LanguageClass mapping
└── census.rs           # SQL-based language census query
```

## Public Functions

| Function | Description |
|----------|-------------|
| `mirror_repo(url)` | Clone or update a repository |
| `mirror_repo_at(url, refspec)` | Clone at a specific branch/tag |
| `repo_census(repo_id)` | Language statistics for a repository |
| `list_repos()` | List all mirrored repositories |
| `drop_repo(repo_id)` | Delete all nodes and local clone |

## Parser Refactoring

Made `parse_go_single`, `parse_c_single`, and `parse_markdown_single` `pub(crate)` with `parent_id: Option<&str>` parameter so file nodes can be parented under repo directory nodes. Rust's `parse_single_file` already accepted `parent_id`.

## Language Detection

Extension-based classification into three categories:
- **Parseable**: `.rs`, `.go`, `.c`, `.h`, `.md` → dispatched to existing parsers
- **OpaqueText**: `.py`, `.js`, `.ts`, `.java`, `.yaml`, etc. → source stored in metadata
- **Binary**: `.png`, `.zip`, `.exe`, etc. → sha256 + size only

Unknown extensions use git's null-byte heuristic on the first 8KB.

## Incremental Updates

On re-mirror: `git2::Diff` between old HEAD and new HEAD trees. Only add/modify/delete changed files. New commits appended without touching existing commit nodes. If HEAD unchanged, returns `{"status": "up_to_date"}`.

## Tests

11 integration tests using `git2::Repository::init()` to create test repos programmatically:

1. `test_mirror_repo_creates_nodes` — verify repo_repository node created
2. `test_commit_nodes_created` — verify commit nodes with sha metadata
3. `test_directory_nodes_created` — verify directory nodes for subdirs
4. `test_parsed_file_has_ast` — verify C file produces c_function nodes
5. `test_opaque_text_file` — verify .py file stored as opaque_text
6. `test_opaque_binary_file` — verify .png stored as opaque_binary
7. `test_repo_census` — verify census JSON structure
8. `test_mirror_idempotent` — second mirror returns up_to_date
9. `test_incremental_update` — new commit detected on re-mirror
10. `test_drop_repo` — all nodes cleaned up after drop
11. `test_list_repos` — repo appears in list

## Verification

```bash
LC_ALL=C CARGO_TARGET_DIR="$(pwd)/tgt" cargo check
LC_ALL=C CARGO_TARGET_DIR="$(pwd)/tgt" cargo pgrx test pg17
```
