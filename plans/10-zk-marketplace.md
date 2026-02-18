# Plan 10: ZK Marketplace

*Depends on: Plan 06 (Distribution), Plan 08 (AI Perspectives)*
*Enables: Plan 11 (External Economy), Plan 20 (ZK Currency)*

## Goal

Implement the zero-knowledge proof layer and Dutch auction mechanism that allow instances to prove they possess valuable knowledge, auction it to interested buyers, release it simultaneously to all successful bidders, and open-source it when the price hits the floor. At the end of this plan, the kerai network has a self-regulating knowledge economy where all knowledge trends toward open.

Plan 20 (ZK Currency) later upgrades the payment layer from plaintext ledger entries to private commitments. This plan is designed to work in both modes — the auction schema, pricing, and settlement logic are the same; only the payment implementation changes.

## The Core Principle: All Knowledge Trends Toward Open

Private knowledge is a temporary state. In a system with vast compute, independent rediscovery is inevitable. The marketplace doesn't fight this — it formalizes the depreciation curve:

1. Knowledge starts private (valuable, scarce)
2. It's attested and auctioned (provably real, price dropping)
3. Buyers pay for early access (competitive advantage)
4. The price hits floor → knowledge goes open to the entire network (joins the Koi Pond)

The auction determines *how long* knowledge stays private and *how much* the producer is compensated. Not *whether* it eventually becomes public — that's a given.

## Deliverables

### 10.1 Dutch Auction Engine

Auction prices are always public — that's how the market functions. Starting price, floor, current price, decrement schedule, and settlement price are plaintext bigints visible to all participants.

```sql
CREATE TABLE kerai.auctions (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attestation_id      UUID NOT NULL REFERENCES kerai.attestations(id),
    seller_wallet       UUID NOT NULL REFERENCES kerai.wallets(id),
    auction_type        TEXT NOT NULL DEFAULT 'dutch',
    starting_price      BIGINT NOT NULL,            -- nKoi (public)
    floor_price         BIGINT NOT NULL DEFAULT 0,  -- nKoi (open-source trigger)
    current_price       BIGINT NOT NULL,            -- nKoi (drops over time)
    price_decrement     BIGINT NOT NULL,            -- nKoi per interval
    decrement_interval  INTERVAL NOT NULL,          -- how often price drops
    min_bidders         INTEGER DEFAULT 1,          -- minimum bidders to trigger settlement
    release_type        TEXT NOT NULL DEFAULT 'simultaneous',
    status              TEXT NOT NULL DEFAULT 'active',
    settled_price       BIGINT,                     -- nKoi (what bidders actually paid)
    open_sourced        BOOLEAN DEFAULT false,
    open_sourced_at     TIMESTAMPTZ,
    signature           BYTEA NOT NULL,             -- signed by seller
    created_at          TIMESTAMPTZ DEFAULT now(),
    settled_at          TIMESTAMPTZ
);

CREATE TABLE kerai.bids (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    auction_id      UUID NOT NULL REFERENCES kerai.auctions(id),
    bidder_wallet   UUID NOT NULL REFERENCES kerai.wallets(id),
    max_price       BIGINT NOT NULL,        -- nKoi (public: the auction needs this)
    funding_proof   BYTEA,                  -- zK proof of sufficient funds (Plan 20, NULL in plaintext mode)
    signature       BYTEA NOT NULL,         -- signed commitment to pay
    created_at      TIMESTAMPTZ DEFAULT now()
);

CREATE INDEX idx_auctions_attestation ON kerai.auctions(attestation_id);
CREATE INDEX idx_auctions_status ON kerai.auctions(status);
CREATE INDEX idx_auctions_seller ON kerai.auctions(seller_wallet);
CREATE INDEX idx_auctions_floor ON kerai.auctions(floor_price);
CREATE INDEX idx_bids_auction ON kerai.bids(auction_id);
CREATE INDEX idx_bids_bidder ON kerai.bids(bidder_wallet);
```

The `funding_proof` column bridges the two modes:

- **Plaintext mode (without Plan 20):** NULL. The system checks `balance >= max_price` directly from the ledger at settlement time. A bidder who overpromises fails at settlement.
- **Private mode (with Plan 20):** A zK proof demonstrating the bidder owns commitments worth >= `max_price`, without revealing total balance. Verified at bid submission time. A bidder cannot bid more than they hold.

**How the Dutch clock works:**

```
starting_price = 80,000 Koi
floor_price = 0
price_decrement = 1,000 Koi
decrement_interval = 1 hour

Hour 0:  current_price = 80,000  (no bidders)
Hour 10: current_price = 70,000  (2 bidders at max_price >= 70,000)
Hour 30: current_price = 50,000  (5 bidders)
Hour 55: current_price = 25,000  (12 bidders, min_bidders met → SETTLE)
  → All 12 bidders pay 25,000. Knowledge released simultaneously.
  → Seller receives 12 × 25,000 = 300,000 Koi.
  → 24 hours after settlement, knowledge goes open to the network.

OR:

Hour 80: current_price = 0 (floor hit, no settlement)
  → Knowledge released to entire network for free.
  → Seller receives 0 Koi but knowledge enters the Koi Pond.
```

The **auction clock** runs as a pgrx background worker — it periodically scans active auctions, decrements prices, checks for settlement conditions, and triggers open-source release when the floor is hit.

### 10.2 Settlement and Simultaneous Release

When settlement triggers (enough bidders at current price, or seller manually triggers):

1. All bidders with `max_price >= current_price` are identified
2. Each pays `current_price` (uniform price — everyone pays the same, regardless of their max bid)
3. Payment is executed according to the active mode:
   - **Plaintext mode:** Signed ledger entries, one per bidder (`from_wallet = bidder, to_wallet = seller, amount = current_price`)
   - **Private mode:** Each bidder's Fuchi generates a transfer proof — nullify input commitments, create a commitment to the seller for `current_price`, create a change commitment to self. The settlement price is public; the bidder's remaining balance and the seller's total holdings are private.
4. The knowledge (operations, perspectives, associations) is disclosed to all winning bidders simultaneously
5. A configurable delay (default 24 hours) after settlement, the knowledge goes open

The simultaneous release is critical — no first-mover advantage among buyers. Everyone who bid gets the knowledge at the same instant. This prevents a buyer from reselling to other bidders at markup.

**Settlement in private mode** means the seller receives N separate commitments, one from each winning bidder, each for `current_price` nKoi. The seller's total receipt is `N × current_price` — calculable from public auction data — but their accumulated balance across all auctions is private.

### 10.3 Open-Source Release

When knowledge goes open (floor hit or post-settlement delay):

1. The attestation's underlying data (perspectives, operations, associations) is published to the network
2. Any instance can sync the data via the normal CRDT operation exchange (Plan 06)
3. The attestation is marked `open_sourced = true` with timestamp
4. The data is now part of the Koi Pond — it syncs freely via normal distribution mechanisms

Open-sourcing is a metadata event, not a ledger transaction. No payment occurs, no commitment is created. The attestation record is updated and CRDT-synced.

```bash
# Browse the Koi Pond (open-sourced knowledge)
kerai market browse --open-sourced --scope "pkg.auth.*"

# Sync open-sourced knowledge from the network
kerai market sync --open-only
```

### 10.4 Zero-Knowledge Proof Generation

Two categories of zK proofs serve different purposes in the marketplace:

**Knowledge proofs** (this plan) — prove properties of knowledge without revealing it:

- **Existence**: "I have a perspective on a node matching pattern X with weight > Y"
- **State transition**: "Applying my operations to your state S produces test result R"
- **Aggregate properties**: "I have N perspectives with average weight > Y covering scope S"

Without revealing: the specific operations, the specific nodes, the actual weights, or the perspective content.

**Currency proofs** (Plan 20) — prove properties of holdings without revealing balances:

- **Funding**: "I own commitments worth >= X nKoi" (for bid funding proofs)
- **Transfer**: "This payment is valid and I had sufficient funds" (for settlement)

Both categories share the same cryptographic infrastructure:

- **Proof system:** PLONK via `arkworks` for complex circuits (knowledge attestation, mint verification). Bulletproofs via `curve25519-dalek` for range proofs (balance conservation, funding sufficiency).
- **Verification:** SQL-callable functions inside the pgrx extension. Same `kerai.verify_proof()` call for both knowledge and currency proofs, dispatched by proof type.
- **Curve:** Curve25519, shared with the Ed25519 identity layer.

```sql
-- Generate a ZK proof for a knowledge attestation
SELECT kerai.generate_knowledge_proof(attestation_id);

-- Generate a ZK funding proof for a bid (Plan 20)
-- (called by Fuchi, not kerai directly)

-- Verify any proof received from another instance
SELECT kerai.verify_proof(proof_type, proof_data);
```

**Knowledge proof generation** runs inside pgrx — the extension has direct access to the data for proof generation, no serialization overhead. The approach:

1. Seller runs the relevant query locally
2. The query execution trace is captured
3. A PLONK proof is generated over the trace, proving the query returned the claimed result on the actual data
4. The proof is stored in `attestations.proof_data` and `challenges.response_proof`
5. Buyers verify the proof without access to the underlying data

This is the hardest part of Plan 10. The schema is ready (`proof_data bytea`); the math is the deliverable.

### 10.5 Challenge Protocol

Extend the `challenges` table from Plan 01 with auction integration:

```sql
ALTER TABLE kerai.challenges ADD COLUMN auction_id UUID REFERENCES kerai.auctions(id);
```

The full protocol:

```bash
# Buyer browses attestations
kerai market browse --scope "pkg.auth.*" --max-price 50000

# Buyer challenges a specific attestation
kerai market challenge <attestation-id> \
  --type state_transition \
  --test "cargo test --package auth" \
  --my-state '{"billy": 147}' \
  --offer 30000

# Seller responds with ZK knowledge proof
kerai market prove <challenge-id>

# Buyer verifies and bids
kerai market bid <auction-id> --max-price 35000
# In private mode: Fuchi generates a funding proof automatically

# Auction settles when conditions are met
# Knowledge is released simultaneously to all winning bidders
```

The `--offer` amount in a challenge follows the same dual-mode pattern:

- **Plaintext mode:** The offer amount is a signed promise, verified against balance at settlement
- **Private mode:** Fuchi generates a funding proof showing the challenger can afford the offer, without revealing total balance

### 10.6 Market Observability

Auction prices, bid counts, and settlement volumes are inherently public — they're market data, not private holdings.

```bash
# Browse active auctions
kerai market browse [--scope path] [--max-price N] [--min-weight N]

# View auction status and bid count
kerai market status <auction-id>

# View your earnings and spending (mode-dependent)
# Plaintext: queries ledger directly
# Private: Fuchi scans commitments with viewing key
fuchi market balance

# Browse the Koi Pond
kerai market commons [--scope path] [--since date]

# Market statistics (all from public data)
kerai market stats
# Output:
#   Active auctions: 47
#   Settled this week: 12 (total value: 450,000 Koi)
#   Open-sourced this week: 8
#   Average settlement price: 37,500 Koi
#   Most active scope: pkg.auth.* (15 auctions)
```

Market stats are derived entirely from public auction data (settlement counts, prices, open-source events). No private commitment data is needed or revealed.

### 10.7 Pricing Strategy Agents

AI agents that help instances set optimal auction parameters:

- Analyze historical settlement data to estimate demand
- Monitor reproduction cost trends (is independent discovery getting cheaper?)
- Recommend starting price, floor price, decrement rate based on knowledge scope and uniqueness
- Track market signals: if nobody bids on `pkg.auth.*` knowledge above 20,000 Koi, stop pricing it at 80,000

This is the point where AI agents participate on both sides of the market — producing knowledge, pricing it, bidding for knowledge they need, and deciding whether to buy or reproduce independently.

## Game Theory

**For buyers:** Bid early at a premium and get the knowledge sooner (competitive advantage over other instances that don't have it). Or wait for the price to drop — but risk someone else reproducing the knowledge independently, or the seller settling with earlier bidders. There's no dominant strategy; the tradeoff depends on how urgently you need the knowledge and how confident you are in independent reproduction.

**For sellers:** Set starting price based on reproduction cost estimate. Set floor based on how quickly you think independent discovery will happen. The market corrects bad estimates — if nobody bids, your reproduction estimate was too optimistic and you should lower prices. If the auction settles quickly at a high price, you underestimated the value.

**For the network:** Every auction resolves in one of two ways — either buyers pay and receive the knowledge (and it goes open shortly after), or the floor is hit and it goes open for free. Either way, the knowledge enters the Koi Pond. The only question is whether the producer gets compensated first. Over time, the Koi Pond grows monotonically. The network gets richer with every auction, whether it settles or not.

**The open-source floor creates a natural pressure toward generosity.** An instance sitting on knowledge and pricing it too high watches the clock tick down toward free. Better to settle at a reasonable price than receive nothing when the floor hits. This keeps the market efficient and prevents knowledge hoarding.

**Privacy and bidding strategy (with Plan 20):** In plaintext mode, other bidders could infer exposure — "this wallet committed 100,000 Koi across three auctions, they might not cover this one." In private mode, that inference is impossible. Bidders can diversify across multiple auctions without revealing their total commitment. This makes the market more efficient (no penalty for diversification) but makes funding proofs essential — without them, a bidder could commit to more than they hold across concurrent auctions.

The funding proof solves this: at bid time, the bidder proves they own sufficient uncommitted funds. If they bid on multiple auctions, each funding proof must reference distinct unspent commitments. Double-committing the same commitment to two bids is prevented by the nullifier mechanism — if both auctions settle, one payment will fail because its input commitment was already nullified by the other.

## Decisions to Make

- **ZK library alignment:** Plan 20 proposes Bulletproofs (range proofs) + PLONK (complex circuits). Knowledge proofs are complex circuits (query execution traces). Proposed: PLONK for both knowledge and mint proofs, Bulletproofs for range/funding proofs. Shared trusted setup, shared verification infrastructure.
- **Post-settlement open delay:** How long after settlement before knowledge goes open to the network? Proposed: configurable per-auction, default 24 hours. This gives buyers a brief exclusive window. Set to 0 for immediate open-sourcing after settlement.
- **Floor price = 0 as default:** Should all auctions eventually go open? Proposed: yes, default `floor_price = 0`. Sellers can set a non-zero floor if they want minimum compensation, but the design ethos is that all knowledge trends toward open.
- **Multi-auction for same knowledge:** Can an instance run multiple auctions for the same attestation (different buyer groups, different price schedules)? Proposed: no, one auction per attestation. Prevents seller from double-dipping. If the auction expires, a new one can be created.
- **Concurrent bid exposure:** In private mode, should funding proofs lock specific commitments (preventing the same Koi from funding multiple concurrent bids)? Proposed: no explicit locking — the nullifier mechanism handles it naturally. If both auctions settle, the second payment fails and the bidder must provide alternative funding or forfeit the bid.

## Out of Scope

- Cross-chain interoperability (Plan 11 handles the USDC bridge)
- Legal framework for knowledge ownership (the system operates on cryptographic proof, not legal IP)
- Reputation systems beyond ledger history (trust scores, reliability ratings — future work)
