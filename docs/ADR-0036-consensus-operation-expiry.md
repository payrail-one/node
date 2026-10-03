# ADR-0036: Height-bound operation expiry without nonce gaps

## Status

Accepted for the Rust reference implementation.

## Context

A merchant checkout can expire in wall-clock time while its signed payment is
delayed in a client, gateway, mempool or recovering node. API expiry alone
cannot prevent a late validator from executing that transfer. Consensus cannot
use each validator's local clock because clock differences would make monetary
execution non-deterministic.

Simply rejecting an expired account-based operation is also unsafe. The
gateway may already have durably allocated its nonce. If that nonce can never
be finalized, every later operation from the account remains blocked behind a
permanent sequence gap.

## Decision

Every transfer and batch carries an inclusive `valid_until_height`. The field
is included in canonical operation bytes, both role-bound signature messages,
the operation ID, the bounded wire envelope, the gateway request digest and the
merchant checkout definition. Changing it therefore creates a different and
unauthorized operation rather than extending an existing payment.

The runtime derives the candidate block height exclusively as the checked
successor of the finalized predecessor. After normal network and signature
verification:

- at a height less than or equal to `valid_until_height`, ordinary monetary
  execution applies;
- at a greater height, the operation moves no amount and charges no fee, but it
  consumes its exact sender nonce, advances the global operation sequence and
  emits an `Expired` receipt and audit event;
- a nonce mismatch or wrong network still fails the block transition.

Consuming the nonce is intentional. It finalizes the signed intent as expired,
prevents later value transfer and unblocks the next sequential operation without
creating an unsigned replacement or silently reusing a reserved nonce.

Receipt persistence carries an explicit `Applied` or `Expired` outcome. The
derived receipt index preserves it across restart, finality reconciliation
stores it in the payment journal, the payment gateway returns a distinct
`Expired` outcome, and merchant checkout exposes `payment_expired` rather than
successful `finalized`.

## Security and operational boundaries

- Wall-clock checkout expiry controls whether a new payer may claim a link.
  Consensus height controls whether the already signed operation may transfer
  value. Both values are immutable checkout fields.
- The service creating a checkout must derive a suitable height from verified
  finalized state and a configured checkout policy. Client-supplied height is
  not trusted without policy validation.
- `valid_until_height == 0` is rejected by the gateway and merchant domain.
  Direct consensus operations still deterministically expire at the first
  positive block height.
- Expired operations consume block capacity without charging a fee. The
  finalized-height admission window, asset-specific fee schedule and proposal
  cleanup quota are defined by ADR-0037. Permissioned admission, bounded pools
  and ADR-0039 process-local tenant/account rate limits reduce exposure; shared
  multi-replica enforcement remains mandatory before public operation.
- Expiry is not cancellation and does not authorize a replacement transaction.
  A retry always uses the exact journaled envelope.
- Existing pre-release journal and checkout records use older internal schema
  markers and fail closed; no production migration promise is made by this
  reference-format change.

## Consequences

Late checkout operations can no longer pay the merchant, yet crash recovery
does not wedge the payer's account nonce. Tests prove that an expired nonce-zero
operation and a valid nonce-one payment execute deterministically in the same
block, only the latter changes balances, expiry is covered by signatures, the
expired outcome survives receipt-index restart, and
checkout status does not misreport expiry as payment success.
