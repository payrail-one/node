# ADR-0021: Deterministic bounded block production

## Status

Accepted for the reference validator application layer.

## Context

Authenticated transaction gossip and deterministic block execution existed as
separate boundaries. A validator still needed a safe rule for selecting a
bounded proposal from its local pool without allowing one stale or temporarily
unfunded transaction to block unrelated payments. Pool entries also must not be
removed merely because a local proposal was built: that proposal may lose its
consensus round.

## Decision

- The gossip pool exposes an immutable bounded candidate snapshot in canonical
  operation-ID order. ADR-0040 selects that subset using local fair traffic
  classes before restoring canonical order. Proposal construction does not
  borrow mutable pool data.
- Candidate identity is recomputed from the canonical signed envelope. Malformed,
  mismatched and permanently invalid authorizations are rejected.
- Valid candidates are scheduled deterministically by sender, nonce and
  operation ID. State-dependent failures remain deferred and receive at most
  four deterministic passes, allowing bounded incoming-payment dependencies
  without unbounded quadratic work.
- Operation count and encoded payload bytes are checked before inclusion. A
  transaction that does not fit remains deferred rather than disappearing.
- The completed payload is executed again through the ordinary block-validation
  path and must reproduce the proposal state byte for byte.
- Preparing a proposal evicts only candidates classified as permanently invalid.
  Included operations stay in the pool until an already verified finalized
  payload is applied. Finality reconciliation is idempotent and fully decodes
  the payload before mutating the pool.
- Pool byte accounting is calculated before removals, so an accounting failure
  cannot leave a partially removed set.

## Consequences

Different validators with the same previous state and candidate set produce the
same payload and state commitment. Different local pools may still produce
different valid proposals; choosing among them remains the responsibility of
the production consensus protocol.

The reference scheduler is correctness-first and monetary mutations remain
sequential. ADR-0022 adds bounded parallel authorization with equivalence tests;
conflict-aware lanes may replace further internals only with equivalent proof
for ordering, inclusion, rejection, receipts and state roots.
