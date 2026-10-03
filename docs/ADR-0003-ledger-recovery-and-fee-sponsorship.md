# ADR-0003: Ledger recovery, receipts and fee sponsorship

Status: accepted for the reference ledger; receipt-in-snapshot and permanent
idempotency-key retention clauses are superseded by ADR-0029

## Context

Payment channels will retry requests when connectivity is poor, merchants need
stable receipts, users must not be forced to acquire a second asset only to pay
a fee, and validator state must be recoverable without trusting an unchecked
database dump.

## Decision

The reference ledger adopts three related controls:

1. Every transfer has a sender-scoped idempotency key and successful operations
   produce a receipt containing the content-derived operation ID, sender, key,
   nonce, monotonic operation index and operation kind. Failed operations
   reserve neither nonce nor key.
2. A sponsored payment requires two explicit authorized origins: the sender and
   the fee payer. The sender pays only the payment amount; the sponsor pays only
   the fee; the configured treasury receives the fee. All balance changes, the
   sender nonce, receipt and event commit atomically.
3. State snapshots use canonical key order and are accepted only after checking
   asset metadata, supply reconciliation, backing rules, account policies and a
   contiguous receipt/nonce history. Event history is not placed in state
   snapshots because it belongs to the append-only chain/block store.
4. Genesis, every payment and every snapshot are bound to an explicit 32-byte
   network domain. A payload or snapshot for another network is rejected before
   state changes.

## Security consequences

- An idempotency key is not a transaction signature or a global transaction
  hash. The signed envelope must bind it to the complete operation payload.
- The operation index is a stable ledger sequence, not an event-log offset.
  Event storage may be rebuilt or pruned without allowing receipt identifiers
  to repeat.
- Sponsor authorization must also cover the complete payload, fee, asset,
  sender nonce and network domain. An API flag such as `sponsored=true` is never
  sufficient authorization.
- A snapshot is state-sync material, not self-authenticating evidence. The node
  layer must bind its canonical encoding to a finalized block hash and verify
  the validator/finality proof before restoration.
- Restoring a snapshot intentionally starts with an empty in-memory event list;
  indexers rebuild historical events from finalized blocks.
- The production runtime may prune old idempotency receipts only under a
  protocol-level retention rule that cannot re-enable replay of a valid signed
  transaction.

## Rejected alternatives

- Charging every user in a separate gas token: unacceptable for national-money,
  merchant and feature-phone UX.
- Letting a gateway silently pay fees without sponsor authorization: creates an
  unbounded treasury-drain path.
- Restoring raw database files without invariant checks: makes corruption or
  tampering part of consensus state.
- Including the entire event history in each snapshot: duplicates the block
  store and makes state sync unnecessarily large.
