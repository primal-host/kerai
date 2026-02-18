# Plan 14: Native Currency

*Depends on: Plan 11 (External Economy), Plan 12 (Kerai Web)*
*Enables: continuous mining, client-side key custody, proportional supply*

## Overview

Turns kerai into a native Postgres-based cryptocurrency where mining is continuous (every verifiable work action auto-mints Koi), external users participate via Ed25519 signed transfers with client-side key custody, and supply grows proportionally with work.

## What Was Implemented

### Schema (Step 1)
- **`kerai.reward_schedule`** — configurable emission rates per work type (parse_file=10, parse_crate=50, parse_markdown=10, create_version=5, bounty_settlement=20, peer_sync=15)
- **`kerai.reward_log`** — audit trail for auto-mints with work_type, reward, wallet_id, details
- **`kerai.wallets.nonce`** — BIGINT column for replay protection on signed transfers

### Currency Module (Step 2) — `src/currency.rs`
9 `pg_extern` functions:
- **`register_wallet(public_key_hex, wallet_type, label?)`** — Accept Ed25519 public key (hex, 64 chars), compute fingerprint, INSERT wallet. No private key touches the server.
- **`signed_transfer(from, to, amount, nonce, signature_hex, reason?)`** — Verify Ed25519 signature over `"transfer:{from}:{to}:{amount}:{nonce}"`, check nonce = wallet.nonce + 1, validate balance, INSERT ledger, increment nonce.
- **`total_supply()`** — Sum all mints. Returns `{total_supply, total_minted, total_transactions}`.
- **`wallet_share(wallet_id)`** — Returns `{wallet_id, balance, total_supply, share}` where share is a decimal string.
- **`supply_info()`** — Rich overview: total_supply, wallet_count, top holders, recent mints.
- **`mint_reward(work_type, details?)`** — Looks up reward_schedule, mints to self instance wallet, logs to reward_log.
- **`evaluate_mining()`** — Periodic evaluation for unrewarded work (retroactive parsing, versions).
- **`get_reward_schedule()`** — List all reward schedule entries.
- **`set_reward(work_type, reward, enabled?)`** — Create or update a reward schedule entry.

### Auto-Mint Hooks (Step 3)
- `parse_crate()` → `mint_reward('parse_crate', ...)`
- `parse_file()` → `mint_reward('parse_file', ...)`
- `parse_source()` → `mint_reward('parse_file', ...)`
- `parse_markdown()` → `mint_reward('parse_markdown', ...)`

### CRDT Op Types (Step 4)
3 new op types in `src/crdt/operations.rs`:
- `register_wallet` — replicate wallet registration
- `signed_transfer` — replicate signed transfers with signature
- `mint_reward` — replicate mint + reward_log entry

### Status Enhancement (Step 5)
`status()` now includes `total_supply` and `instance_balance` fields.

### CLI Commands (Steps 8-9)
`kerai currency <subcommand>`:
- `register` — --pubkey, --type, --label
- `transfer` — --from, --to, --amount, --nonce, --signature, --reason
- `supply` — show total supply info
- `share` — wallet_id positional
- `schedule` — list reward schedule
- `set-reward` — --work-type, --reward, --enabled

## Tests Added (17 new, 140 total)
- `test_register_wallet_currency` — register with valid hex pubkey
- `test_register_wallet_invalid_key` — #[should_panic] on bad hex
- `test_register_wallet_duplicate_key` — #[should_panic] on same pubkey
- `test_signed_transfer` — register, mint, sign, transfer, verify balances
- `test_signed_transfer_bad_signature` — #[should_panic]
- `test_signed_transfer_bad_nonce` — #[should_panic] on replay
- `test_signed_transfer_insufficient_balance` — #[should_panic]
- `test_total_supply` — mint, verify total
- `test_wallet_share` — mint, verify share calculation
- `test_supply_info` — verify rich supply overview
- `test_mint_reward` — call mint_reward, verify ledger + reward_log
- `test_mint_reward_disabled` — disable work_type, verify null return
- `test_evaluate_mining` — verify periodic evaluation
- `test_get_reward_schedule` — verify 6 seed entries
- `test_set_reward` — create/update reward entry
- `test_auto_mint_on_parse` — parse_source triggers supply increase
- `test_status_includes_supply` — status JSON has total_supply + instance_balance

## Key Design Decisions
1. **Client-side key custody**: `register_wallet` accepts a public key hex string. The server never sees or stores private keys.
2. **Signed transfers**: Message format `"transfer:{from}:{to}:{amount}:{nonce}"` — deterministic, nonce provides replay protection.
3. **Proportional supply**: Total supply grows continuously with work. No inflation schedule or halving.
4. **Configurable reward schedule**: Instance owners tune emission rates per work type. Defaults seeded at extension creation.

## Files Changed
| File | Action | Description |
|---|---|---|
| `src/schema.rs` | Modified | reward_schedule, reward_log tables + seed data; wallets nonce column |
| `src/currency.rs` | Created | 9 pg_extern functions |
| `src/parser/mod.rs` | Modified | Auto-mint hooks in parse_crate/parse_file/parse_source |
| `src/parser/markdown/mod.rs` | Modified | Auto-mint hook in parse_markdown |
| `src/crdt/operations.rs` | Modified | 3 new op types + apply handlers |
| `src/functions/status.rs` | Modified | total_supply + instance_balance in status JSON |
| `src/lib.rs` | Modified | mod currency + 17 tests |
| `cli/src/commands/currency.rs` | Created | CLI currency subcommands |
| `cli/src/commands/mod.rs` | Modified | Currency module + Command variants |
| `cli/src/main.rs` | Modified | CurrencyAction enum + dispatch |
