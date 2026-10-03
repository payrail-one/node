# ADR-0012: Finality-proof verification boundary

## Status

Accepted as a stack-neutral reference verifier and test seam. It is not a new
consensus protocol and does not replace a Polkadot SDK GRANDPA justification.

## Context

State sync already requires an opaque finality proof before accepting a
snapshot manifest. The node now needs a concrete, adversarially testable
boundary for network, membership epoch, validator-set hash, checkpoint and
signature quorum while the final Polkadot SDK integration is still pending.

## Decision

1. `finality-proof-core` defines a canonical weighted validator set. Validators
   are strictly ordered by stable node ID; consensus keys are unique; weights
   are non-zero and summed with checked arithmetic.
2. The validator-set hash binds network, membership epoch, ordered node IDs,
   consensus keys and weights.
3. A reference certificate binds network, membership epoch, round, height,
   block hash, state root and validator-set hash. Signatures are strictly
   ordered and unique by node ID.
4. Every supplied signature is verified. Unknown validators, malformed extra
   signatures and invalid signatures fail the whole certificate even if an
   otherwise sufficient subset exists.
5. Acceptance requires strictly more than two thirds of configured voting
   weight. For seven equal validators this is five signatures, so one or two
   offline validators preserve liveness while three do not.
6. The certificate codec is bounded by the existing 1 MiB state-sync finality
   proof limit and rejects truncation, trailing bytes and allocation above the
   validator bound before processing.
7. `FinalityVerifier` directly implements the state-sync
   `FinalityProofVerifier` interface. A snapshot checkpoint differing in any
   field is rejected.
8. Strict Ed25519 verification remains a separate adapter. Production
   consensus signing keys belong in HSM/remote signers and are distinct from
   peer transport and account keys.

Polkadot SDK separates block production from provable finality through GRANDPA,
and exposes GRANDPA finality information through its runtime APIs:
[Polkadot consensus documentation](https://docs.polkadot.com/reference/polkadot-hub/consensus-and-security/pos-consensus/),
[runtime API documentation](https://docs.polkadot.com/chain-interactions/query-data/runtime-api-calls/).

## Security boundary

The reference certificate proves only that the configured keys signed the exact
checkpoint under the reference domain. By itself it does **not** prove:

- block ancestry or fork choice;
- safety/liveness of a voting protocol;
- absence or handling of equivocation;
- validator-set transition correctness;
- catch-up across historical authority-set changes;
- that signers independently executed and validated the block.

Those properties come from the selected consensus implementation and its
audited justification verifier. When Polkadot SDK is integrated, the production
adapter must verify authentic GRANDPA justifications and authority-set
transitions rather than translating them into weaker ad-hoc votes.

## Current evidence

- Seven independent Ed25519 validator keys are divided across three modeled
  failure domains.
- Five-of-seven and six-of-seven certificates verify; four-of-seven fails.
- Forged signatures, duplicate/reordered/unknown signers, wrong network, stale
  epoch, wrong validator set and changed checkpoint fail closed.
- Canonical proof bytes connect directly to the state-sync verification trait.

This is deterministic cryptographic evidence, not yet a distributed consensus
latency or finalized-TPS benchmark.
