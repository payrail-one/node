# ADR-0020: Canonical signed-operation block execution

## Status

Accepted for the Rust reference runtime and recovery harness.

## Context

The tail-sync prototype previously derived state from synthetic payload bytes.
That proved finality, ordering and crash-safe publication, but it did not prove
that recovered blocks could execute real authenticated payments or reproduce a
canonical ledger state on every validator.

## Decision

- A ledger block is a bounded, ordered list of canonical signed-operation
  envelopes. Empty blocks are valid; oversized blocks, operation counts,
  envelopes, trailing bytes and unsupported values are rejected before state
  publication.
- The runtime restores the previous canonical snapshot through the existing
  ledger invariant validator and verifies its state root before execution.
- Operations execute in consensus order against an isolated candidate ledger.
  Ed25519 signatures, network binding, nonce, replay, balance, fee, asset and
  account-policy rules use the existing authoritative ledger paths.
- If any operation fails, the entire candidate block is discarded. Only a
  completely successful block produces receipts, a canonical next-state image,
  block hash and state root.
- Canonical state serialization and normalized database rows share one record
  representation. The same reconstructed state must pass monetary, supply,
  backing, replay and canonical-order validation.
- `tail-sync-core` remains runtime-neutral. `ledger-runtime-core` implements its
  transition boundary, and the LMDB adapter receives only a verified prepared
  transition.

## Consequences

The multi-process recovery harness now downloads a real canonical ledger
snapshot, rejects a corrupt provider chunk, resumes after restart, verifies
three finalized blocks containing real signed payments and atomically commits
their normalized state to LMDB through height 103.

Monetary execution remains deliberately sequential. Authorization checks are
now bounded and parallel above a configured block threshold, while ordered
capability consumption preserves byte-identical receipts, state roots, error
precedence and whole-block failure atomicity. See ADR-0022. Conflict-aware
transaction scheduling and consensus/storage pipelining remain future work.

This is a reference payment runtime, not yet a production consensus or smart
contract VM. Contract execution requires a separate deterministic metering,
sandbox, ABI and upgrade decision.
