# Plan 11: External Economy

*Depends on: Plan 05 (CLI), Plan 10 (ZK Marketplace), Plan 14 (ZK Currency)*
*Enables: human and AI participation in the knowledge economy beyond kerai instances*

## Goal

Bridge the private Koi economy to the external world — USDC exchange, price discovery, and fiat on/off ramps. Inside the kerai network, Koi transactions are fully private (Plan 14). The bridge is the sole point where privacy is deliberately sacrificed. At the end of this plan, humans can enter the Koi economy (buying Koi with USDC), participate privately (holding, transferring, commissioning work), and exit (converting Koi back to USDC) — with privacy preserved everywhere except at the bridge boundary.

## Why This Is Inevitable

The Koi is grounded in verifiable compute work. It has real utility — you spend it to acquire knowledge that would cost more to reproduce independently. Any token with real utility and verifiable scarcity attracts outside interest:

- **Humans** who commission knowledge work want to pay for it directly
- **Investors** who believe certain knowledge domains will increase in value want exposure
- **AI systems** operating outside the kerai network want to purchase knowledge from inside it
- **Instances** that produce valuable knowledge want to convert Koi to fiat to cover infrastructure costs (electricity, hosting, API fees)

Ignoring this creates a shadow economy. Planning for it channels the inevitable into something coherent.

## The Privacy Boundary

Plan 14 establishes that all Koi transactions are private — balances are Pedersen commitments, transfers are zK proofs, double-spending is prevented by nullifiers. Nobody reading the database can determine who holds what or who sent what to whom.

The external world (USDC, exchanges, fiat) is inherently transparent. The bridge is where these two worlds meet:

```
Private Pond                    Bridge                     External World
─────────────                  ──────────                  ──────────────
Commitments, proofs            Amount revealed             USDC on L2
Nullifiers                     Identity tied               Exchange trades
Encrypted memos                KYC possible                Fiat on/off ramps
                               Audit trail
```

This is a deliberate, user-initiated choice. Nobody is forced to bridge. You can earn, hold, transfer, and spend Koi entirely within the private pond. The bridge exists for those who want external liquidity.

## Deliverables

### 11.1 Fuchi Wallet

Fuchi (Plan 14.7) is the client-side wallet that manages private Koi. The external economy extends it with bridge operations:

```bash
# Create a wallet (spending key, viewing key, commitment inventory)
fuchi wallet create --label "billy-personal"
# Output: wallet ID, public key, fingerprint
# Keys saved to ~/.fuchi/keys/

# Check balance (scans private ledger with viewing key)
fuchi wallet balance
# Output computed locally — kerai never sees your total

# Transfer Koi (generates zK proof client-side)
fuchi transfer --to <recipient-pubkey> --amount 5000
# Fuchi builds commitments, generates proof, submits to kerai

# View transaction history (decrypts memos with viewing key)
fuchi history [--since date]

# Export wallet (encrypted backup of keys + commitment inventory)
fuchi wallet export --output wallet-backup.enc

# Import wallet
fuchi wallet import wallet-backup.enc

# Recover wallet (scan ledger with viewing key to rebuild commitment inventory)
fuchi wallet recover
```

**Key difference from Plan 11 original:** Balance is computed locally by Fuchi, not queried from Postgres. Transfer builds a zK proof, not a signed ledger row. History decrypts memos, not reads plaintext.

### 11.2 Wallet Types

The `wallets` table (Plan 01) supports four types, now with privacy implications:

| Type | Owner | Privacy | Typical Use |
|------|-------|---------|-------------|
| `instance` | A kerai instance | Shielded | Internal knowledge economy, minting |
| `human` | A person | Shielded | Holding Koi, commissioning work, bridge exit/entry |
| `agent` | An AI agent | Shielded | Autonomous trading, portfolio management |
| `bridge` | Bridge operator | Transparent on external side | USDC exchange, the privacy boundary |

All wallet types use Ed25519 keypairs and operate in the shielded domain by default. The `bridge` wallet is special: it holds reserves as commitments internally, but its external side (USDC holdings, mint/burn events) is publicly visible on the L2 chain.

### 11.3 USDC Bridge

The bridge connects the private Koi pond to USDC on an Ethereum L2. It is the sole mechanism for moving value between the two worlds.

**Exit (Koi to USDC):**

```
1. User proves in zK they own a commitment worth X nKoi
2. User reveals the amount X to the bridge (deliberate privacy sacrifice)
3. Bridge verifies the proof and nullifies the commitment on the kerai side
4. Bridge releases X equivalent USDC from reserves (or mints wrapped KOI)
5. User receives USDC in their L2 wallet — publicly visible, tied to external identity
```

```bash
# Exit: convert Koi to USDC
fuchi bridge exit --amount 10000 --to 0xABC...DEF
# Fuchi generates a proof of ownership, reveals the amount,
# and submits to the bridge. USDC sent to the L2 address.
```

**Entry (USDC to Koi):**

```
1. User sends USDC to the bridge contract on the L2
2. Bridge verifies the deposit (on-chain confirmation)
3. Bridge creates a fresh shielded commitment on the kerai side
4. User receives the commitment in Fuchi — private from this point forward
```

```bash
# Entry: convert USDC to Koi
fuchi bridge enter --amount 10000 --from 0xABC...DEF
# Monitors the L2 for deposit confirmation, then creates
# a shielded commitment. The last point of visibility.
```

**What the bridge sees:** the amount and the external identity (L2 address). **What the bridge doesn't see:** the user's total Koi balance, their other commitments, or their transaction history inside the pond.

**Bridge reserves:** The bridge holds Koi commitments on the kerai side and USDC on the L2 side. The total USDC in reserves equals the total Koi that has exited minus what has re-entered. This is publicly auditable on-chain.

**Bridge custody:** Multi-sig requiring N-of-M instance signatures. Proposed: start with a single trusted operator (the kerai network), add multi-sig when multiple independent instances participate.

**External chain:** Ethereum L2 — Base (Coinbase L2) for initial deployment. Low fees, mature USDC support, large existing DeFi ecosystem. The bridge contract is a standard ERC-20 with mint/burn controlled by the bridge operator.

### 11.4 Selective Disclosure

Some economic activities require public amounts to function. The zK layer supports selective disclosure — revealing specific values while keeping everything else private:

**Auctions (Plan 10):** Starting price, current price, floor price, and settlement price are public. This is how the market functions — buyers need to see prices. But the seller's total balance, the bidders' total balances, and all non-auction transfers remain private.

**Bounties:** The reward amount is public (to attract workers). The poster proves they can cover the reward without revealing their total balance. When the bounty settles, the worker receives a shielded commitment.

```bash
# Post a bounty with selective disclosure
fuchi bounty create \
  --scope "pkg.auth.*" \
  --success-criterion "cargo test --package auth" \
  --reward 50000
# Fuchi proves the poster can fund 50,000 Koi (revealed)
# without disclosing total balance (private)
```

**Supply auditing:** Individual balances are private, but aggregate supply is verifiable. Every mint proof declares its amount publicly (tied to verifiable work metrics). Total minted = sum of all mint proof amounts. This is a public counter, not derived from hidden commitments.

```bash
# Audit total supply (walks the mint proof chain)
kerai supply audit
# Output: total minted (provable), total bridged out, total bridged in
# Does NOT reveal individual balances
```

### 11.5 Commissioning Work

Humans (or external AIs) with wallets can commission knowledge production. Bounty rewards are selectively disclosed; all other wallet state remains private.

```bash
# Post a bounty
fuchi bounty create \
  --scope "pkg.auth.*" \
  --success-criterion "cargo test --package auth" \
  --reward 50000

# Instances and agents see the bounty, compete to fill it
kerai bounty list --open

# When an instance fills the bounty, payment is automatic:
# - Success criterion is verified
# - Poster's funding commitment is nullified
# - Worker receives a shielded commitment for the reward amount
# - The bounty amount (50,000 Koi) was publicly known
# - The worker's resulting balance is not
```

```sql
CREATE TABLE kerai.bounties (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    poster_wallet     UUID NOT NULL REFERENCES kerai.wallets(id),
    scope             ltree NOT NULL,
    description       TEXT NOT NULL,
    success_command   TEXT,                      -- automated verification
    reward            BIGINT NOT NULL,           -- nKoi (selectively disclosed)
    funding_proof     BYTEA NOT NULL,            -- zK proof poster can cover reward
    funding_nullifier BYTEA,                     -- nullified when bounty is funded
    status            TEXT NOT NULL DEFAULT 'open',
    claimed_by        UUID REFERENCES kerai.wallets(id),
    payout_commitment BYTEA,                     -- shielded commitment to worker
    verified_at       TIMESTAMPTZ,
    signature         BYTEA NOT NULL,
    created_at        TIMESTAMPTZ DEFAULT now(),
    expires_at        TIMESTAMPTZ
);

CREATE INDEX idx_bounties_scope ON kerai.bounties USING gist(scope);
CREATE INDEX idx_bounties_status ON kerai.bounties(status);
CREATE INDEX idx_bounties_poster ON kerai.bounties(poster_wallet);
CREATE INDEX idx_bounties_reward ON kerai.bounties(reward);
```

### 11.6 Speculation and Price Discovery

Once Koi is bridgeable to USDC, the exchange rate reflects collective belief about future knowledge production value. The bridge creates a natural market:

- Exit demand (Koi → USDC) indicates instances monetizing knowledge earnings
- Entry demand (USDC → Koi) indicates external actors buying into the economy

**Market statistics** are published without compromising individual privacy:

```bash
kerai market export-stats --format json
# Output:
#   total_minted: sum of all mint proof amounts (provable)
#   total_bridged_out: cumulative Koi exited via bridge
#   total_bridged_in: cumulative USDC entered via bridge
#   auction_settlements_30d: count and total volume
#   active_bounties: count and total reward pool
#   mint_rate_30d: Koi minted per day (from work proofs)
#   active_instances: number of minting instances
```

These aggregates come from public data: mint proofs (public amounts), bridge events (public by design), auction settlements (public prices), and bounty postings (selectively disclosed rewards). No private commitment data is revealed.

**Domain-specific futures** remain possible via external DeFi once the token is liquid. Kerai exposes the data; external protocols build the instruments.

### 11.7 Revenue Model for Instances

The path from knowledge to fiat gains a privacy-preserving step:

```
Instance produces knowledge
  → Knowledge sold via Dutch auction (Plan 10, public price)
  → Koi earned as shielded commitment (private)
  → Koi transferred within the pond (private)
  → User proves ownership at bridge, reveals amount (deliberate disclosure)
  → Bridge releases USDC (public)
  → USDC sold for fiat
  → Fiat pays for hosting, compute, API costs
```

The key insight: the instance's total earnings, balance, and transfer history remain private. Only the specific amount being bridged is revealed, and only at the moment of bridging. An instance could bridge small amounts frequently or large amounts infrequently — the pattern is invisible to outside observers (they see only the bridge events, not the internal flow).

AI systems have the same path — an autonomous agent that earns Koi can use them to purchase compute (API calls, GPU time) by bridging to USDC, or spend Koi directly within the pond for knowledge from other instances.

### 11.8 Fiat On-Ramp

For humans who want to buy Koi without running an instance:

```bash
# Buy KOI (wrapped ERC-20) on an external exchange
# Then bridge it into the private pond:
fuchi bridge enter --amount 10000 --from 0xABC...DEF
```

The on-ramp is the last point of visibility. Once USDC converts to a shielded Koi commitment, the holder's activity is private. They can commission bounties (selectively disclosing reward amounts), bid in auctions (bid amounts are public), or transfer Koi to other wallets (fully private).

No custom fiat integration needed initially — the external exchange ecosystem handles USDC/fiat conversion. The bridge handles USDC/Koi conversion. Fuchi handles the privacy transition.

## Tokenomics

### Supply

- **No pre-mine — provably.** Every Koi traces to a mint proof tied to verified work. The mint circuit (Plan 14.3) guarantees this cryptographically. There is no way to mint without a valid work proof. No founder's allocation, no VC tokens, no genesis block with pre-distributed coins.
- **Inflationary.** New Koi are minted as work is done. The inflation rate is bounded by the amount of actual compute happening in the network. More work = more Koi, but also more knowledge produced and more utility backing each Koi.
- **Deflationary pressure from open-sourcing.** As knowledge goes open (Dutch auction floor), the exclusive value it backed returns to the Koi Pond. Koi spent acquiring that knowledge don't disappear, but the knowledge that gave them utility is now free. This creates natural deflationary pressure.
- **Auditable aggregate, private individual.** Total supply is provable by summing all mint proof amounts (public). Individual holdings are private (shielded commitments). External observers can verify total supply and mint rate without seeing who holds what.

### Demand

- **Utility demand:** Instances need Koi to query other instances, acquire knowledge, and participate in auctions.
- **Speculative demand:** External actors believe the knowledge economy will grow, increasing future demand for Koi.
- **Commissioning demand:** Humans and external AIs post bounties that require Koi.
- **Privacy demand:** Koi offers private value transfer that USDC does not. Some participants will hold Koi specifically for its privacy properties.

### Equilibrium

The Koi price in USDC should converge toward the average cost of compute per unit of knowledge produced. If Koi trades above this, it's cheaper to produce knowledge than buy Koi — new instances enter the market, increasing supply. If Koi trades below, it's cheaper to buy Koi than to compute — instances reduce production, decreasing supply. The market self-regulates around compute cost parity.

The bridge enforces this: unlimited exit (anyone can convert Koi to USDC at the market rate) means the Koi price can't sustainably diverge from its utility value. Overvaluation invites sell pressure; undervaluation invites buy pressure from those who need knowledge.

## Decisions to Make

- **L2 chain:** Base is proposed for initial deployment (Coinbase L2, mature USDC, low fees). Alternatives: Arbitrum, Optimism. One chain first, expand later.
- **Bridge operator model:** Single trusted operator initially, or multi-sig from the start? Multi-sig is more secure but complex. Proposed: single operator (the kerai network itself), add multi-sig as the network grows.
- **Exchange rate mechanism:** Fixed peg (1 Koi = $X USDC, adjusted periodically)? Or market-determined (AMM pool on the L2)? Proposed: AMM pool — let the market set the rate. The bridge provides liquidity; Uniswap (or equivalent) provides the mechanism.
- **Minimum network maturity:** Should there be a minimum network size before enabling the bridge? Proposed: yes. The internal economy should prove itself before external speculation enters. Launch the bridge when the network has 100+ active instances and meaningful transaction volume.
- **Regulatory considerations:** Utility tokens have different regulatory treatment than securities in most jurisdictions. The Koi is clearly a utility token (spent on knowledge queries). Privacy features add complexity. Legal review recommended before exchange listing.

## Out of Scope

- Building a DEX (use existing AMMs — Uniswap, Aerodrome, etc.)
- Derivatives and futures contracts (external DeFi protocols handle this)
- Fiat banking integration beyond USDC (standard exchange on/off ramps suffice)
- Regulatory compliance framework (requires legal counsel, jurisdiction-specific)
- Governance tokens / DAO structure (the economy is algorithmic, not governed by vote)
- Bridges to currencies other than USDC (one bridge first, others follow the same pattern)
