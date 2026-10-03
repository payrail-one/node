# ADR-0026: Height-gated authenticated checkpoint root

## Status

Accepted for the reference runtime, LMDB adapter and recovery harness. ADR-0027
binds the policy into signed bootstrap configuration. Production activation
still requires governed config updates, multi-validator compatibility evidence,
retention rules and independent review.

## Context

ADR-0024 and ADR-0025 made the normalized Jellyfish Merkle Tree deterministic,
persistent and atomic with ledger finalization. Finalized checkpoints still
committed to a SHA-256 hash of the complete canonical state image. Replacing
that root implicitly would make old and upgraded validators finalize different
checkpoints at the same height.

The migration must be deterministic, one-way, visible in network
configuration, enforced by both execution and storage, and safe for a node
restored from a snapshot on either side of the activation height.

## Decision

- `StateCommitmentPolicy` selects `CanonicalStateV1` below an optional
  activation height and `AuthenticatedStateV1` at and above that height.
- The policy never falls back after activation. A network that does not set an
  activation height retains the original commitment indefinitely.
- The block executor verifies the previous checkpoint with the scheme assigned
  to its height and computes the next checkpoint root with the scheme assigned
  to the next height. The activation block therefore links a legacy-root parent
  to an authenticated-root child without changing block encoding.
- Proposal construction and finalized transition verification use the same
  executor policy. A validator with a different policy rejects the transition.
- Ledger snapshot initialization validates the snapshot checkpoint under the
  configured height policy. LMDB persists a checksummed policy record in the
  same transaction as ledger initialization.
- An existing ledger refuses to open under a different policy. Stores created
  before this record existed are migrated only to the legacy, no-activation
  policy; an implicit authenticated-root activation is rejected.
- At authenticated heights, LMDB compares the incrementally prepared JMT root
  with the finalized checkpoint root before publishing state, tree nodes, block
  record and cursor in one transaction.
- Canonical state images remain stored for snapshot export and deterministic
  recovery. Changing the checkpoint commitment does not remove full-state
  validation or the normalized ledger invariants.

## Evidence

Runtime tests execute the boundary from height 100 to activation height 101,
continue at height 102, and prove that a legacy-configured executor rejects the
authenticated checkpoint. The LMDB integration test crosses the same boundary,
reopens the environment, rejects a mismatched policy, commits another signed
payment and verifies retained proofs.

The process recovery harness restores a finalized snapshot at height 100,
rejects a corrupt provider, resumes persisted chunks, then verifies and commits
three real signed payment blocks with authenticated checkpoint roots through
height 103 before reopening LMDB.

With authenticated checkpoint roots enabled from block one, three comparable
local 1,000-transfer pipeline runs produced 7,447.75, 7,700.10 and 7,653.81
transfers/s. The median was 7,653.81 transfers/s. This result excludes network
finality and is not a production throughput or payment-latency claim.

## Consequences

The reference stack now has an explicit compatibility path from the original
full-state root to proof-capable authenticated roots. ADR-0027 binds the
activation height into signed bootstrap configuration and its protocol digest;
dynamic governance changes still require an explicit update protocol.

ADR-0028 adds a deterministic two-phase incremental execution view without
trusting storage-produced roots. The stateless complete-rebuild path remains an
equivalence oracle and fallback. Historical tree pruning, archive retention,
remote multi-validator compatibility and rollback/recovery drills remain
required before production activation.
