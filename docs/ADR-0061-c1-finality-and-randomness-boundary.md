# ADR-0061: C1 finality and randomness boundary

## Status

Accepted for the pinned C1 testnet epoch-0 integration. Independent finality,
transaction-inclusion and successful-execution verification are implemented and
have verified a live signed transfer. Validator-set transitions and an on-chain
Random.one draw proof remain open.

## Context

Payrail needs to submit a ticket purchase to the C1/R1 network and independently
confirm that the corresponding transaction finalized successfully. Trusting the
HTTPS server or a JSON `success` field would make the serving node an implicit
custodian and would not prove BFT finality.

C1 exports four proof layers:

- a production Commit QC for a finalized height;
- a discoverable validator-set snapshot;
- transaction inclusion against the finalized header's transaction root;
- successful execution against the v2 header's receipts root.

The validator-set response is discovery data, not an authority. The existing
`/api/randomone/draw` demo also uses server-side randomness and is not currently
bound to a finalized transaction or event. It cannot support a provably-fair
claim.

## Decision

1. `c1-finality-adapter` implements the C1 `FinalityProofV1` wire contract
   independently of the C1 codebase. It verifies:
   - proof/set version 1 and bounded JSON input;
   - pinned chain ID, genesis hash, epoch and validator-set hash;
   - canonical validator and vote order, unique keys and non-zero stake;
   - the `C1RBFT:VALIDATOR-SET:v1` BLAKE3 commitment;
   - every Ed25519 Commit signature over the exact C1 vote digest;
   - the non-configurable production quorum `floor(total_stake * 2 / 3) + 1`;
   - checked stake arithmetic and configured validator/vote limits.
2. The same adapter independently reproduces C1's canonical transaction and
   v1/v2 block-header hashes. It verifies the transaction's zero-padded Merkle
   path against the finalized header and verifies the successful-receipt leaf
   against the v2 receipts root. All nested proof layers and their bounds are
   checked in one operation. Until the C1 proof JSON represents other lanes,
   Payrail submits only Standard-lane ticket transactions.
3. The HTTP validator-set snapshot never updates the trust anchor by itself.
   Epoch changes fail closed until Payrail verifies a C1 epoch-transition proof
   or an operator installs a separately authenticated anchor.
4. Public Nginx routing exposes only constrained GET paths for finality,
   validator-set, transaction-inclusion and execution proofs. The authenticated
   loopback proxy and general C1 admin API remain private.
5. A lottery ticket is a Payrail application record linked to the exact signed
   R1 transaction. The ticket becomes confirmed only after Payrail verifies
   finality, inclusion and successful execution. Finality alone is insufficient.
6. C1 proof verification is asynchronous and outside Payrail's high-throughput
   payment-consensus critical path. A stalled C1 endpoint can delay ticket
   confirmation but cannot reduce Payrail ledger safety or block unrelated
   payments.
7. Randomness acceptance remains disabled until Random.one exposes a finalized
   draw event or transaction whose exact bytes can be checked through inclusion
   and successful-execution proofs. The current demo draw endpoint is display
   data only.

## Payrail and RND asset flow

Payrail does not issue the PR balance as an R1-native asset. PR remains an
accounted Payrail balance and is the user-facing payment instrument. RND is the
settlement and game asset used by the R1 transaction.

The demo uses a fixed test exchange rate and pre-funded PR and RND liquidity.
Each phone identity maps to a Payrail account and a test R1 wallet. A ticket
purchase follows one recoverable workflow:

1. reserve the quoted PR amount and persist an idempotent ticket order;
2. allocate RND from the pre-funded demo inventory;
3. submit the signed RND ticket transaction to R1;
4. verify finality, inclusion and successful execution;
5. settle the PR reservation and mark the ticket confirmed, or release the
   reservation if the R1 operation reaches a terminal failure.

The production integration does not perform a visible market purchase for each
ticket. Payrail reserves PR and sends an authenticated, replay-safe payment
intent directly to the R1 adapter. The adapter pays the ticket from a shared,
pre-positioned RND settlement pool and returns independently verifiable R1
execution. Pool replenishment, conversion and counterparty settlement happen
separately on a net basis. The production pool uses reconciled hot/cold RND
inventory and HSM/MPC-backed signing. The workflow must durably record every
state transition and use the ticket order ID as its replay boundary. A lost
response must be recovered by looking up the submitted R1 transaction, never by
blindly charging or sending again.

The demo pool is funded with faucet-issued test RND. Initial production RND can
be allocated by the Random.one treasury, a jointly funded Payrail/R1 operating
account, or a contracted liquidity provider; it is not assumed to be an operator's
personal capital. Commercial terms define ownership, minimum inventory,
replenishment thresholds and the net compensation Payrail owes for RND consumed
by settled PR purchases. The settlement pool is operational liquidity and must
remain segregated from the lottery jackpot and customer balances.

Winnings use the reverse path: verify the successful R1 payout, credit the
user's Payrail balance after conversion from RND, then optionally pay out
through an approved mobile-money, bank or PSP off-ramp. Real-money funding,
conversion and withdrawal require the applicable licensed partners, KYC/AML,
sanctions checks and jurisdiction-specific gaming controls. These production
controls are not simulated as if they already exist in the test-credit demo.

## Dependency review

The adapter adds exact pins for `bincode 1.3.3`, `blake3 1.8.7`, `serde 1.0.229`,
`serde_json 1.0.151`, `thiserror 2.0.20` and the workspace-standard
`ed25519-dalek 3.0.0`. These crates are RustCrypto/Serde ecosystem components
under permissive MIT/Apache-2.0 or compatible licensing. No HTTP client is added
to the security-critical library; transport remains an outer adapter.

## Verification evidence

Targeted tests cover valid 3-of-4 production quorum, wrong chain/genesis/epoch,
wrong validator-set anchor, sub-quorum, invalid signature, duplicate/unknown
voter, non-canonical validator order, duplicate key, zero stake, arithmetic
overflow, unsupported version, genesis height, JSON bounds and unknown fields.
Inclusion/execution tests additionally cover canonical transaction and header
binding, key material, Merkle bounds and tampering, v2 receipt commitment,
nested-finality tampering and rejection of uncommitted receipts.

On 2026-09-23 the independent checker verified the live public C1 testnet proof
for chain 2, epoch 0, height 6759 and block
`86f76f597334d38ab5bd6852c173b64fcb0d3b027b84407b96ff2991e8ba3f55`.
Signed stake was `3,000,000,001 / 4,000,000,001`, satisfying the production
threshold. Public validator-set/finality GET routes returned 200 and POST was
denied. This proves live finality compatibility.

Later the same day, Payrail's signed-transfer faucet submitted transaction
`2917cf644099d1581f64a97306dcdeef606d9b7f0bb41e2a54e4a698c0ec15df`.
The network finalized it at height 1664 and the recipient's independently read
balance increased by `1,000,000` atomic units. `c1-proof-check execution`
verified its exact v2 header, transaction inclusion at `0/1`, successful receipt
at `0/1`, and Commit QC with signed stake `3,000,000,001 / 4,000,000,001`.
The sequencer remained healthy with zero restarts. This closes the live
transaction/inclusion/execution compatibility gate; it does not prove randomness
fairness, production readiness or throughput.

## Next gate

Exercise one idempotent, test-credit SMS ticket flow using the now-verified C1
submission and proof boundary: accepted command, signed Standard-lane R1
transaction, confirmed ticket and replay-safe recovery. Fiat, Paynow and EcoCash
are explicitly outside this demo gate.
