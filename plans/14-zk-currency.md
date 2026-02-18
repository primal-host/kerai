# Plan 14: ZK Currency

*Depends on: Plan 01 (Foundation), Plan 10 (ZK Marketplace)*
*Enables: Plan 11 (External Economy)*

## Goal

Replace the plaintext Koi ledger with a zero-knowledge proof layer where balances, transfers, and minting are fully private inside the kerai network. External currencies (USDC) are exchanged at a bridge that is the sole point where privacy is deliberately sacrificed. At the end of this plan, Koi transactions are cryptographically private, verifiable without revealing amounts or participants, and bridgeable to the external world only by explicit user choice.

## The Plaintext Layer (What Exists)

The plaintext Koi currency is already implemented. It works as a complete currency system on its own. This plan upgrades it from plaintext to private — transfers become zK proofs, replay protection moves from nonces to nullifiers, and balances become Pedersen commitments. Both modes coexist; users choose when to shield.

### Schema

- **`kerai.reward_schedule`** — configurable emission rates per work type, amounts in nKoi:
  - parse_file = 10,000,000,000 (10 Koi)
  - parse_crate = 50,000,000,000 (50 Koi)
  - parse_markdown = 10,000,000,000 (10 Koi)
  - create_version = 5,000,000,000 (5 Koi)
  - bounty_settlement = 20,000,000,000 (20 Koi)
  - peer_sync = 15,000,000,000 (15 Koi)
  - model_training = 25,000,000,000 (25 Koi)
  - mirror_repo = 100,000,000,000 (100 Koi)
- **`kerai.reward_log`** — audit trail for auto-mints with work_type, reward (nKoi), wallet_id, details
- **`kerai.wallets.nonce`** — BIGINT column for replay protection on signed transfers

### Currency Module — `src/currency.rs`

9 `pg_extern` functions:
- **`register_wallet(public_key_hex, wallet_type, label?)`** — Accept Ed25519 public key (hex, 64 chars), compute fingerprint, INSERT wallet. No private key touches the server.
- **`signed_transfer(from, to, amount, nonce, signature_hex, reason?)`** — Verify Ed25519 signature over `"transfer:{from}:{to}:{amount}:{nonce}"`, check nonce = wallet.nonce + 1, validate balance, INSERT ledger, increment nonce. Remains for plaintext-mode transfers.
- **`total_supply()`** — Sum all mints. Returns `{total_supply, total_minted, total_transactions}`. Unaffected by shielding — mint amounts are always public (tied to verifiable work metrics).
- **`wallet_share(wallet_id)`** — Returns `{wallet_id, balance, total_supply, share}` where share is a decimal string. Only operates on unshielded balances. Shielded balances are invisible to the server — only Fuchi knows them.
- **`supply_info()`** — Rich overview: total_supply, wallet_count, top holders, recent mints. "Top holders" reveals only plaintext-mode balances. The private ledger provides aggregate-only alternatives that don't disclose individual holdings.
- **`mint_reward(work_type, details?)`** — Looks up reward_schedule, mints to self instance wallet, logs to reward_log. After this plan, minting creates a commitment instead of a plaintext ledger entry. The hook and reward_log stay the same; the ledger target changes.
- **`evaluate_mining()`** — Periodic evaluation for unrewarded work (retroactive parsing, versions). Bonus amounts use `NKOI_PER_KOI` constant (1 Koi per node, capped at 100 Koi).
- **`get_reward_schedule()`** — List all reward schedule entries.
- **`set_reward(work_type, reward, enabled?)`** — Create or update a reward schedule entry.

### Auto-Mint Hooks

- `parse_crate()` → `mint_reward('parse_crate', ...)`
- `parse_file()` → `mint_reward('parse_file', ...)`
- `parse_source()` → `mint_reward('parse_file', ...)`
- `parse_markdown()` → `mint_reward('parse_markdown', ...)`

These hooks stay the same — the mint target changes from plaintext ledger to commitment, but the trigger mechanism is identical.

### CRDT Op Types

3 op types in `src/crdt/operations.rs`:
- `register_wallet` — replicate wallet registration (unchanged by this plan)
- `signed_transfer` — replicate signed transfers with signature (plaintext mode only; the private ledger replicates proof + commitment + nullifier instead)
- `mint_reward` — replicate mint + reward_log entry (changes to commitment + mint proof)

### Key Design Decisions

1. **Client-side key custody**: `register_wallet` accepts a public key hex string. The server never sees or stores private keys. This aligns directly with Fuchi's model — the private key stays client-side, extended with viewing keys and commitment inventory.
2. **Signed transfers**: Message format `"transfer:{from}:{to}:{amount}:{nonce}"` — deterministic, nonce provides replay protection. The private ledger uses nullifiers instead — a fundamentally different replay protection mechanism suited to the commitment model.
3. **Proportional supply**: Total supply grows continuously with work. No inflation schedule or halving. Mint proofs still tie supply growth to verifiable work. Individual mint amounts become commitments while aggregate supply remains publicly auditable.
4. **Configurable reward schedule**: Instance owners tune emission rates per work type. Defaults seeded at extension creation. All amounts in nKoi.
5. **Curve25519 alignment**: Ed25519 signed transfers operate on Curve25519 — the same curve used by Bulletproofs for range proofs and balance conservation. The cryptographic foundation is shared.

### Files Implementing the Plaintext Layer

| File | Action | Description |
|---|---|---|
| `src/schema.rs` | Modified | reward_schedule, reward_log tables + seed data (nKoi); wallets nonce column |
| `src/currency.rs` | Created | 9 pg_extern functions, `NKOI_PER_KOI` constant |
| `src/parser/mod.rs` | Modified | Auto-mint hooks in parse_crate/parse_file/parse_source |
| `src/parser/markdown/mod.rs` | Modified | Auto-mint hook in parse_markdown |
| `src/crdt/operations.rs` | Modified | 3 new op types + apply handlers |
| `src/functions/status.rs` | Modified | total_supply + instance_balance in status JSON |
| `src/lib.rs` | Modified | mod currency + 17 tests |
| `cli/src/commands/currency.rs` | Created | CLI currency subcommands |
| `cli/src/commands/mod.rs` | Modified | Currency module + Command variants |
| `cli/src/main.rs` | Modified | CurrencyAction enum + dispatch |

## The Core Principle: Privacy Inside the Pond

Koi never leaves the kerai network. It doesn't need a public blockchain, distributed consensus, or gas fees. The ledger is Postgres. The trust boundary is the kerai instance (or the CRDT network of instances). This collapses the complexity of traditional zK crypto:

| Traditional zK Crypto | Koi zK |
|---|---|
| Prove to adversarial, trustless network | Prove to kerai instances |
| On-chain proof verification by miners | Postgres extension verifies directly |
| Gas costs per verification | No gas — server-side compute |
| Global consensus required | CRDT sync between trusted instances |
| Trusted setup ceremony with public | Trusted setup controlled by network |

The only point where privacy breaks is the bridge to external currencies. Converting Koi to USDC is a *choice* — the user deliberately steps from the private pond into the transparent world. This mirrors how cash works: private until you deposit it in a bank.

## Architecture

```
Fuchi (client)           kerai (server)           Bridge (boundary)
─────────────           ──────────────           ─────────────────
Generate keypair        Verify proofs            Reveal amount
Build commitments       Store commitments        Exchange Koi <> USDC
Generate zK proofs      Track nullifiers         Identity tied here
Encrypt memos           CRDT-sync commitments    Public ledger here
                        Mint with work proofs
```

## Deliverables

### 14.1 Commitment Scheme

Replace plaintext amounts with Pedersen commitments. A commitment `C = v*G + r*H` hides the value `v` behind a random blinding factor `r`. Nobody reading the database can determine balances.

**Current plaintext ledger:**

```sql
-- Anyone with database access sees everything
SELECT from_wallet, to_wallet, amount, reason FROM kerai.ledger;
```

**Private ledger:**

```sql
CREATE TABLE kerai.private_ledger (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    commitment      BYTEA NOT NULL,       -- Pedersen commitment (hides amount)
    nullifier       BYTEA UNIQUE,         -- prevents double-spend (NULL for mints)
    proof           BYTEA NOT NULL,       -- zK proof of validity
    encrypted_memo  BYTEA,                -- only sender+receiver can decrypt
    epoch           BIGINT NOT NULL,      -- ordering (replaces lamport timestamp)
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_private_ledger_nullifier ON kerai.private_ledger (nullifier);
CREATE INDEX idx_private_ledger_epoch ON kerai.private_ledger (epoch);
```

Each entry proves: the sender owned sufficient Koi, the amounts balance (inputs = outputs + fee), no coins were created from nothing, and no double-spending occurred — all without revealing sender, receiver, or amount.

### 14.2 Private Transfers

A transfer from Alice to Bob:

1. **Fuchi (Alice's client)** selects unspent commitments totaling >= transfer amount
2. Fuchi builds a new commitment for Bob (amount `v`, fresh blinding factor `r`)
3. Fuchi builds a change commitment for Alice (remaining balance, fresh blinding factor)
4. Fuchi generates a zK proof that:
   - The input commitments exist and haven't been spent (nullifier check)
   - The sum of inputs >= sum of outputs (no inflation)
   - All amounts are non-negative (range proof)
5. Fuchi encrypts a memo (amount, note) that only Bob can decrypt
6. The proof, commitments, nullifiers, and encrypted memo are sent to kerai
7. kerai verifies the proof and stores the new ledger entries
8. CRDT sync replicates commitments and nullifiers across instances

**What kerai sees:** valid proof, opaque commitments, nullifiers. **What kerai doesn't see:** who sent what to whom, or how much.

### 14.3 Private Minting

When kerai mints Koi for work (parsing, perspectives, bounties), it needs to prove:

- The work actually happened (e.g., N nodes were parsed)
- The reward matches the schedule (e.g., parse_file = 10 Koi = 10,000,000,000 nKoi)
- The mint was authorized by a valid instance

Without revealing: which specific files were parsed, the content of the work, or which wallet received the reward.

```
Work proof:
  "I parsed files producing N nodes.
   The reward schedule says 10 Koi per file.
   I minted N * 10 Koi to a wallet I control.
   Here's a proof."
```

The mint creates a new commitment with no input nullifier (coins from nothing, like a coinbase transaction). The proof ties the mint amount to verifiable work metrics without exposing the work itself.

This matters for CRDT sync: instance B can verify instance A's mints are legitimate without seeing A's private data. The proof is the receipt.

### 14.4 Nullifier Set

Double-spend prevention without revealing which commitment is being spent:

- Each unspent commitment has a secret nullifier known only to the owner
- When spent, the nullifier is published
- kerai maintains a nullifier set — if a nullifier appears twice, the second transaction is rejected
- The zK proof demonstrates the nullifier corresponds to a real unspent commitment without revealing which one

```sql
-- Fast nullifier existence check
CREATE TABLE kerai.nullifiers (
    nullifier   BYTEA PRIMARY KEY,
    epoch       BIGINT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

The nullifier set grows monotonically. It syncs via CRDT across instances — a nullifier published on any instance is eventually known to all.

### 14.5 Proof System

**Proposed: Bulletproofs** for range proofs and balance verification.

- No trusted setup — critical since we want the privacy guarantees to be unconditional
- Logarithmic proof size — efficient for range proofs (proving amounts are non-negative)
- Rust implementation available (`bulletproofs` crate, `curve25519-dalek`)
- Already uses the same curve as our Ed25519 identity layer (Curve25519)

For more complex proofs (minting verification, work attestation), **PLONK** via `arkworks`:

- Universal trusted setup (one-time, reusable across circuits)
- Efficient for arithmetic circuits (balance equations, schedule lookups)
- Mature Rust implementation

The split: Bulletproofs for transfer range proofs (no setup), PLONK for mint/work proofs (universal setup controlled by the network).

### 14.6 USDC Bridge

The bridge is where privacy ends and external liquidity begins.

```
Private Pond                    Bridge                     External World
─────────────                  ──────────                  ──────────────
Koi (commitments)  ──reveal──> Amount + Identity ──mint──> USDC (ERC-20)
                                                           on L2
Koi (commitments)  <──lock───  Amount + Identity <──burn── USDC (ERC-20)
```

**Koi to USDC (exit):**

1. User proves in zK they own a commitment worth X nKoi
2. User reveals the amount X to the bridge (deliberate privacy sacrifice)
3. Bridge nullifies the commitment on the kerai side
4. Bridge mints X equivalent USDC on the external chain (or releases from reserves)
5. The user now holds USDC — publicly visible, tied to their external identity

**USDC to Koi (entry):**

1. User burns/locks USDC on the external chain
2. Bridge verifies the burn transaction
3. Bridge creates a fresh commitment on the kerai side for the equivalent nKoi
4. The user receives the commitment — from this point forward, fully private

**Bridge custody:** A multi-sig bridge wallet requiring N-of-M instance signatures. The bridge holds Koi reserves (as commitments) and USDC reserves (on-chain). Exchange rates are set by the bridge operator or by market mechanisms.

**The privacy boundary is clean:** inside the pond, everything is commitments and proofs. The bridge is the only place where amounts become visible, and only because the external world requires it.

### 14.7 Wallet State (Fuchi)

Fuchi manages the client-side secrets that make privacy work:

- **Spending keys** — Ed25519 private keys (already have these)
- **Viewing keys** — derived keys that can decrypt memos without spending authority
- **Commitment inventory** — the set of unspent commitments the wallet owns
- **Blinding factors** — random values used in each commitment (must be stored to spend)
- **Nullifier preimages** — secrets needed to construct nullifiers when spending

```
~/.fuchi/
├── keys/
│   ├── spending.key      -- Ed25519 signing key (encrypted at rest)
│   └── viewing.key       -- derived viewing key
├── commitments/
│   └── unspent.db        -- SQLite: commitment, amount, blinding, nullifier_preimage
└── config.toml
```

Loss of the spending key means loss of funds (same as any crypto wallet). Loss of the viewing key means loss of transaction history but not funds. The commitment inventory can be reconstructed by scanning the ledger with the viewing key.

### 14.8 Migration Path

The current plaintext ledger continues to work. The private ledger runs alongside it. Users choose when to "shield" their Koi:

1. **Shield:** Transfer from plaintext ledger to a commitment (reveals amount during shielding, private afterward)
2. **Unshield:** Reveal a commitment back to the plaintext ledger (deliberate de-privacy)
3. **Private transfer:** Commitment to commitment (fully private)
4. **Plaintext transfer:** Current ledger behavior (unchanged)

Over time, as Fuchi matures, the plaintext ledger becomes the legacy path and shielded becomes the default. No forced migration — users opt in to privacy.

## Plaintext vs Private

| Aspect | Plaintext (current) | Private (this plan) |
|--------|--------------------|---------------------|
| Amounts | Visible in ledger | Hidden in commitments |
| Transfers | Ed25519 signature | zK proof |
| Replay protection | Nonce (sequential) | Nullifier (one-time) |
| Balance query | `SUM(amount)` from ledger | Fuchi scans with viewing key |
| Minting | Plaintext ledger entry | Commitment with mint proof |
| CRDT replication | Amount + signature | Proof + commitment + nullifier |
| Supply audit | `SUM(amount) WHERE from_wallet IS NULL` | Sum of mint proof amounts (public) |

Users shield Koi by transferring from the plaintext ledger to a commitment (14.8). They unshield by revealing a commitment back. Both layers share the same denomination (nKoi), the same reward schedule, and the same curve (Curve25519).

## Proof Circuits

### Transfer Circuit

Proves: "I'm spending commitments worth >= X nKoi and creating new commitments worth exactly X nKoi (plus change)."

```
Public inputs:
  - nullifiers[]           (published, checked against nullifier set)
  - output_commitments[]   (new commitments added to ledger)

Private inputs (known only to prover):
  - input_amounts[]        (value of each input commitment)
  - input_blindings[]      (blinding factors for input commitments)
  - output_amounts[]       (value of each output commitment)
  - output_blindings[]     (blinding factors for output commitments)
  - nullifier_preimages[]  (secrets that produce the nullifiers)
  - merkle_paths[]         (proof each input commitment exists in the ledger)

Constraints:
  1. sum(input_amounts) = sum(output_amounts)          -- conservation
  2. each input_amount >= 0                             -- range proof
  3. each output_amount >= 0                            -- range proof
  4. each nullifier = hash(nullifier_preimage)          -- valid nullifiers
  5. each input_commitment = pedersen(amount, blinding) -- commitment validity
  6. each merkle_path proves commitment exists           -- existence
```

### Mint Circuit

Proves: "This instance performed work of type T producing metric M, and the reward schedule maps T to R nKoi per unit."

```
Public inputs:
  - output_commitment     (new minted commitment)
  - work_type_hash        (hash of work type, e.g., hash("parse_file"))
  - metric                (e.g., number of files parsed)

Private inputs:
  - amount                (R * metric, in nKoi)
  - blinding              (commitment blinding factor)
  - reward_rate           (from schedule: nKoi per unit of work)
  - work_proof            (instance-specific proof that work occurred)

Constraints:
  1. amount = reward_rate * metric                      -- correct reward
  2. output_commitment = pedersen(amount, blinding)     -- commitment validity
  3. work_type_hash = hash(work_type)                   -- matches claimed type
  4. verify(work_proof) = true                          -- work actually happened
```

## Denomination

All amounts inside commitments are denominated in nKoi (nano-Koi). 1 Koi = 1,000,000,000 nKoi. This is the same convention used by the plaintext ledger. The commitment hides the nKoi value; the bridge reveals it when exiting to USDC.

## CRDT Sync Implications

Commitments and nullifiers sync between instances via CRDT:

- **Commitments** are append-only — new ones are added, never modified
- **Nullifiers** are append-only — once published, permanent
- **Proofs** travel with their commitments — any instance can verify
- **Encrypted memos** are opaque to non-participants

The CRDT merge is straightforward: union of commitments, union of nullifiers. No conflicts possible — both sets are append-only and deduplicated by content.

A receiving instance verifies each proof before accepting it into its local ledger. Invalid proofs are rejected at sync time, preventing inflation attacks from rogue instances.

## Decisions to Make

- **Proof system finalization:** Bulletproofs for range proofs + PLONK for mint circuits, or a single system for everything? Bulletproofs have no trusted setup but are slower to verify. PLONK is faster but needs a universal setup.
- **Merkle tree for commitment set:** What hash function and tree structure? Poseidon hash is zK-friendly (cheap inside circuits). A sparse Merkle tree allows efficient membership proofs.
- **Viewing key derivation:** How to derive viewing keys from spending keys? BIP-32-style hierarchical derivation, or simpler Diffie-Hellman shared secret (like Zcash's in-band secret distribution)?
- **Bridge operator model:** Single trusted bridge operator initially, or multi-sig from the start? Multi-sig is more secure but more complex. Proposed: start with single operator (the kerai network itself), add multi-sig when multiple independent instances participate.
- **USDC chain:** Which L2 for the USDC side of the bridge? Base, Arbitrum, and Optimism all have low fees and mature USDC support.

## Out of Scope

- Full anonymity set analysis (statistical deanonymization resistance — future work)
- Regulatory compliance framework for the bridge (KYC/AML — depends on jurisdiction and bridge operator model)
- Cross-chain bridges to currencies other than USDC (one bridge first, others follow the same pattern)
- Hardware wallet integration for spending key custody
- Fuchi client implementation (separate plan — this plan defines what Fuchi needs to do; the Fuchi plan covers how)
