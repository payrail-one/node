# ADR-0023: Atomic bounded local batch ingress

## Status

Accepted for the reference API-to-mempool boundary.

## Context

The original pool API decoded and verified one local transaction per call.
That is appropriate for request-first peer gossip, but it serializes signature
verification for API batches, exchange deposits, payroll and high-throughput
merchant traffic. It also makes aggregate capacity behavior an application
concern and can partially admit a caller batch before a later entry exceeds the
pool limit.

Batching must not weaken canonical decoding, network binding, per-envelope
reporting, signature checks, bounded memory or independent block validation.

## Decision

- Local ingress accepts at most 4,096 envelopes per call. Peer-to-peer gossip
  remains request-first and continues to admit one requested envelope at a time.
- Every input is canonically decoded and network-checked before validation.
  Malformed, wrong-network and signature-rejected entries receive deterministic
  outcomes indexed to the caller's original order.
- `TransactionValidator` has a batch method with a sequential default. The
  reusable `transaction-ingress-core` adapter invokes the same bounded parallel
  authorization implementation used by the block runtime.
- Validation must return exactly one result for every decoded transaction. An
  internal verifier failure or result-cardinality mismatch rejects the call and
  leaves the pool unchanged.
- Existing and within-batch operation identifiers are idempotent duplicates.
  Only unique, newly valid envelopes participate in aggregate capacity checks.
- Operation-count and byte capacity are calculated with checked arithmetic
  before mutation. If the complete new valid set does not fit, every member of
  that set receives the same capacity rejection and none is inserted.
- The pool is mutated only after the complete ordered report and next byte count
  have been constructed. There is no fallible operation after insertion begins.
- Admission is not a consensus trust signal. Proposal reproduction and every
  received finalized block still decode and independently verify the wire
  payload through the normal runtime executor.

## Consequences

Tests cover mixed malformed/wrong-network/rejected/valid input, existing and
within-batch duplicates, atomic count and byte exhaustion, validator failure,
bad result cardinality, oversize rejection and real Ed25519 batch admission.

On the same preliminary 1,000-transfer local pipeline scenario, three runs
produced 11,767.51, 9,935.87 and 10,022.43 transfers/s. The median was
**10,022.43 transfers/s**, 39.22% above the prior parallel-runtime median and
134.91% above the initial sequential baseline. This remains a local engineering
measurement without network finality, multi-node contention, WAN or HSM delay.
