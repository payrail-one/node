# ADR-0059: Preverified authorization and compact envelope reconciliation

## Status

Accepted. The bounded process-local authorization cache, its ingress/runtime
integration and deterministic local payload reconstruction from the bounded
gossip pool are implemented. The Malachite proposal stream now sends a
proposer-signed manifest before the full-payload fallback, and a fully connected
permissioned committee can select direct one-hop broadcast instead of a gossip
mesh. Missing-envelope request/response integration over the authenticated
validator transport and its sustained client-to-finality benchmark remain open.

## Context

The live three-validator Malachite profile finalizes a 12,288-operation block in
285.49 ms at the median on six shared Linux vCPUs. Reaching 65,000 transfers/s
requires no more than 189.05 ms per maximum block. Instrumentation identifies
two overlapping critical phases: authenticated delivery of the four-megabyte
payload and independent Ed25519 verification/execution at each validator.

High-throughput systems avoid making consensus repeatedly distribute data that
validators already received:

- [Narwhal](https://arxiv.org/abs/2105.11827) separates reliable transaction
  dissemination from ordering and reports more than 130,000 tx/s in its WAN
  Narwhal-HotStuff evaluation;
- [Aptos Quorum Store](https://medium.com/aptoslabs/quorum-store-how-consensus-horizontally-scales-on-the-aptos-blockchain-988866f6d5b0)
  continuously disseminates transaction batches, persists them and uses a
  2f+1 availability proof so consensus orders compact batch metadata;
- [Bitcoin BIP-152](https://github.com/bitcoin/bips/blob/master/bip-0152.mediawiki)
  reconstructs a compact block from transactions already in the receiver's
  mempool and requests only missing transactions before normal validation;
- [Mysticeti](https://docs.sui.io/paper/mysticeti.pdf) demonstrates that a DAG
  design can exceed 200,000 TPS with roughly 0.5-second WAN commit latency, but
  replacing the selected consensus engine is not required for the current
  Payrail bottleneck.

Payrail already has bounded request-first transaction gossip, full 32-byte
operation IDs in the compact manifest and fail-closed pre-vote execution. The
missing bridge is to retain the local result of ingress authorization and use
the already-gossiped exact envelopes during proposal reconstruction.

## Decision

### Process-local authorization capability cache

`ledger-core` derives `VerifiedAuthorizationId` from a domain-separated hash of:

1. the complete canonical operation, which includes network and all monetary
   fields;
2. the sender identity and complete sender signature;
3. the presence, identity and complete signature of the fee payer.

This differs deliberately from `OperationId`, which is independent of
signatures. A modified authorization for the same operation cannot hit the
cache.

Only the existing private-construction `VerifiedOperation` capability may enter
the cache. It has no wire or storage codec and is created only after the local
authoritative verifier succeeds. Every lookup recomputes the full signed
identity and compares the complete `SignedOperation` after the hash match. A
hypothetical hash collision is therefore a miss, not an authorization bypass.

The cache is:

- bound to one `NetworkId`;
- process-local and empty after restart;
- bounded independently by operation count and encoded authorization bytes;
- FIFO-evicted so remote retries cannot keep entries resident indefinitely;
- optional: wrong-network, absent, oversized or unavailable cache state falls
  back to the ordinary strict verification path.

The initial defaults are 16,384 operations and 64 MiB of encoded authorization
material. They match the current bounded mempool scale without becoming a new
unbounded queue.

`ParallelTransactionValidator` records a capability only after applying current
admission policy and local cryptographic verification. `LedgerBlockExecutor`
may reuse the same process-local capability when it decodes the byte-identical
operation from a proposed block. On every cache hit it still reapplies, in
consensus order:

- nonce and replay/idempotency rules;
- balances, amounts, fees and checked arithmetic;
- expiry and block-height policy;
- account and asset policy;
- block hash, receipts, state transition and state root.

Consensus peers never send or assert cache hits. Each validator independently
creates its own capabilities.

### Exact-envelope reconciliation

The implementation keeps Malachite and the current compact manifest. Before a
proposal, validators gossip canonical signed envelopes and verify them
independently. On receiving the proposal manifest a validator:

1. resolves every ordered full operation ID from its bounded local pool;
2. reconstructs and checks the exact payload length and payload hash;
3. requests only missing full envelopes from authenticated admitted validators;
4. uses bounded peers, bytes, outstanding requests, deadlines and retries;
5. executes the fully reconstructed payload before voting;
6. rejects without voting if reconstruction or execution is incomplete.

The existing signed full-payload chunk stream remains the fallback for a
partially overlapping pool or a round whose missing-envelope deadline expires.
It is not removed until live reconciliation has fault and restart evidence.

The proposal-parts protocol authenticates the compact descriptor with a new
`ManifestSeal` before exposing it to an optional local reconstructor. A returned
payload enters the complete-only store only after the ordinary exact manifest
binding check. A miss continues consuming the signed `Data` chunks and
`AvailabilitySeal`; the final value is never delivered to consensus early.

`block-production-core::reconcile_payload` now implements the complete local
case. It walks manifest order rather than pool order, returns a bounded ordered
list of missing full IDs without exposing a partial payload, reconstructs via
the authoritative canonical block codec and requires exact payload length,
full signed-payload hash and ordered operation IDs. A different signature for
the same operation ID therefore fails the manifest commitment. The missing-ID
result is the input to the still-open authenticated transport driver.

Full 32-byte operation IDs remain mandatory. BIP-152-style short IDs would make
the manifest smaller, but require collision detection, ambiguity handling and
DoS limits. That optimization is deferred because the current 393,280-byte
manifest is not the primary measured bottleneck.

An Aptos-style quorum availability certificate may later allow consensus to
order only batch digests. Such a certificate requires durable storage before a
validator signs its promise, an expiry policy and a committee-size analysis.
It is not silently approximated by an in-memory flag.

## Security consequences

- Signature verification is moved earlier, not removed.
- A cache miss always invokes the existing verifier; cache failure cannot make
  an invalid operation valid.
- Parent-dependent and monetary validity is never cached.
- Exact full-envelope identity prevents operation-ID substitution and modified
  signature reuse.
- Every validator still needs the complete payload and successful deterministic
  execution before voting.
- Finality verification and atomic LMDB publication are unchanged.
- A malicious proposer can at worst force bounded missing-envelope work or a
  failed round; it cannot finalize unavailable or unexecuted data.

## Verification and remaining gate

Focused tests prove exact cache reuse, modified-signature misses for the same
operation ID, network isolation, count/byte eviction, invalid-limit rejection
and fallback verification. A runtime test proves a cache hit avoids repeat
cryptography while a replay against the next state is still rejected by the
nonce rule. Reconciliation tests prove exact manifest-order reconstruction,
bounded missing-ID reporting without partial publication and rejection of a
different signature sharing the same operation ID.

A native M1 Max release diagnostic over three maximum blocks measured a
240,688.49 transfers/s median with strict in-block verification and a
592,700.17 state-transition transfers/s median after exact local
preverification, reducing median block p50 from 51.22 to 20.58 ms. The separate
preverification phase took a 190.99 ms median for 36,864 operations. This is
evidence that the chosen seam shortens proposal-time execution; it is not
evidence that cryptographic CPU work or network dissemination disappeared.

The three-validator steady-state profile now proves that the shortened critical
path exceeds the target. Three valid 6-vCPU Linux/arm64 runs with independently
preverified process-local caches, no payload preload, live 4,005,908-byte block
streams, direct permissioned broadcast, deterministic execution, BFT finality
and every validator's durable LMDB commit measured 93,047.54 / 96,832.62 /
98,059.14 finalized transfers/s; the median was 96,832.62. Median run-level p50
and p95 proposal-to-durable-finality were 121.39 ms and 136.71 ms.

This closes only the explicitly named steady-state consensus-capacity gate.
Operations and their Ed25519 authorization were admitted before the timed first
proposal, so the result does not include client submission, transaction gossip
or preverification CPU in the interval. The full client-to-finality 65,000 gate
still requires a sustained pipelined benchmark that overlaps those stages and
proves that none falls behind. The JSON labels both preverification and any
payload preload; a payload-preloaded run cannot be used for this evidence.
