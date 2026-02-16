# Kerai (ker.ai)

AST-based version control backed by Postgres, using CRDTs for conflict-free convergence, with cryptographic identity and a built-in knowledge economy.

From 家来 (kerai) — Japanese for "staff." AI agents that work alongside developers.

## Status

Design phase. See [plans/](plans/) for implementation plans and [conversation-kerai-design.md](conversation-kerai-design.md) for the design conversation.

## Architecture

Kerai is a [pgrx](https://github.com/pgcentralfoundation/pgrx) extension — it runs *inside* Postgres, not alongside it.

```sql
CREATE EXTENSION kerai;
```

That's it. Schema, crypto identity, DSL functions, background workers — all created by the extension. The CLI is a thin client. Any Postgres client is a kerai client.

## Plan Sequence

| Plan | Title | Summary |
|------|-------|---------|
| 01 | Foundation | Postgres schema, crypto identity, economic ledger |
| 02 | Rust Parser | Parse Rust source into the database via `syn` |
| 03 | Source Reconstruction | Reconstruct source files via `prettyplease` + `rustfmt` |
| 04 | CRDT Operations | Operation log, version vectors, deterministic merge |
| 05 | CLI | The `kerai` command-line tool |
| 06 | Distribution | Clone, push, pull, cross-instance sync |
| 07 | Code Queries | Structural query language for code insight |
| 08 | AI Perspectives | Weighted nodes, agent views, knowledge valuation |
| 09 | Agent Swarms | Massive concurrent agents, test-driven convergence |
| 10 | ZK Marketplace | Zero-knowledge proofs, Dutch auctions, open-source floor |
| 11 | External Economy | Wallets, token bridge, bounties |

## Origin

Originally named "astute" from a naming brainstorm around AST-related words. Renamed to kerai when the project's focus crystallized around AI agents serving developers. See [musings.md](musings.md) for the naming history.
