# ADR-0035: Durable single-use merchant checkout

## Status

Accepted for the Rust reference implementation.

## Context

A payment link or point-of-sale request must not create a new monetary intent on
every retry. Mobile clients, merchant terminals and provider callbacks can all
repeat a request after a timeout. The checkout also spans two independently
durable components: merchant-order storage and the journal-first payment
gateway. They currently use separate LMDB environments, so they cannot share
one database transaction.

Treating publication as payment completion would let a merchant fulfil an order
that is not finalized. Letting a second payer replace the first after a timeout
could also create two valid transfers for one order.

## Decision

Introduce a transport-independent `merchant-checkout-core` domain and a
network-bound `merchant-checkout-lmdb` adapter.

A checkout is a fixed-amount, single-use request. Its identity is derived from
the network, tenant, merchant and merchant order key. The mutable request body
is deliberately excluded from identity: an exact retry returns the stored
checkout, while changing the asset, amount, destination, fee policy or expiry
for the same order fails with a definition conflict.

The first valid payment attempt atomically claims the checkout for exactly one
payer and fee selection. The same LMDB transaction persists the claim and adds
a pending gateway-dispatch row. A different payer or fee can never replace it.
Exact retries remain valid after checkout expiry so recovery cannot abandon an
already claimed payment.

The claim derives one stable gateway client key from the checkout ID. Dispatch
uses the existing journal-first payment gateway:

1. a crash before gateway submission leaves durable pending checkout work;
2. a crash during submission is recovered by the gateway journal;
3. a crash after gateway success but before checkout acknowledgement repeats
   the identical gateway request instead of signing a replacement;
4. gateway acknowledgement and pending-row removal are committed together in
   the checkout environment.

The checkout store validates its immutable order-owner and pending-dispatch
secondary indexes both on reopen and on record reads. Pending work is exposed
through a bounded exclusive-cursor page. One dispatch failure is retained on
its item and does not prevent later items in the page from running.

Merchant-visible state is derived from both stores. `Accepted` means the exact
operation was published; only an indexed finalized receipt produces
`Finalized`. A terminal or merchant system must not release goods merely on an
API acknowledgement or mempool admission.

## Security and operational boundaries

- The checkout ID is a locator, not an authorization secret. Merchant mutation
  and private status endpoints require an authenticated tenant and merchant
  principal; payment requires authenticated wallet authorization.
- Wall-clock expiry prevents a new checkout claim, while immutable
  `valid_until_height` is signed and enforced by deterministic consensus
  execution. A late operation finalizes as `Expired`, transfers no value and
  consumes its nonce; see ADR-0036.
- Claim and gateway submission are intentionally recoverable rather than
  cross-database atomic. The durable dispatch row plus exact gateway
  idempotency closes both crash windows without distributed transactions.
- LMDB coordinates processes on one host. Concurrent dispatch is safe through
  exact gateway idempotency, but a production service still needs a bounded
  scheduler, fenced ownership, backoff and telemetry for this queue.
- A claimed checkout cannot be cancelled, reassigned or edited. Refunds are
  separate compensating payments with their own authorization and audit trail.
- Reusable/static QR requests, variable amounts, FX quotes, partial payments,
  tips, webhooks and refund state machines are outside this increment.

## Consequences

Merchant order retries now have one durable monetary identity, and a process
restart cannot silently lose a payer claim or cause replacement signing. Tests
cover immutable creation, tenant isolation, expiry and fee boundaries, restart
recovery, bounded dispatch, exact retry without a second signature or publish,
and the distinction between accepted and finalized payment state.
