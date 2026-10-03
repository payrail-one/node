# ADR-0031: Journal-first payment idempotency and finality reconciliation

## Status

Accepted for the Rust reference implementation.

## Context

Consensus nonces stop the ledger from executing the same account sequence twice,
but they do not make a merchant, wallet, SMS or USSD API request idempotent. A
caller can lose the response before learning whether a nonce was allocated, an
operation was signed, or a broadcast reached a validator. Creating a replacement
operation after a timeout can therefore produce a duplicate payment or a nonce
conflict.

Finalized receipts were intentionally removed from live consensus state and are
available through the independently rebuildable receipt index. The gateway needs
a separate durable request journal that can correlate a client request with the
exact signed operation and then reconcile it against that finality evidence.

## Decision

Introduce four separate capabilities:

- `payment-gateway-core` defines the canonical simple-payment request and the
  policy/nonce/signer/verifier/journal/publisher orchestration order;
- `payment-idempotency-core` owns the storage-independent request identity and
  monotonic state machine;
- `payment-idempotency-lmdb` atomically persists reservations and account-wide
  nonce ownership;
- `payment-reconciliation-core` advances a submitted request only from verified
  finalized-receipt-index evidence.

A request identity is the tuple `(tenant, authenticated account, client request
key)`. Its immutable intent also binds the network, canonical request digest and
ledger idempotency key. An exact retry returns the existing record. Reusing the
same identity with different immutable data fails closed.

The canonical request digest covers the network, tenant, payer, client key,
asset, recipient, integer amount, fee and optional fee payer. A separate
domain-separated hash derives the ledger idempotency key from that digest and
request identity. Published golden vectors prevent accidental format drift.

The state machine is:

```text
Reserved -> NonceAssigned -> SubmissionPrepared -> Published -> Finalized
                                  |              ^          ^
                                  v              |          |
                             Indeterminate ------+----------+

Reserved -> Rejected
```

Policy validation and rejection must happen before nonce allocation. Once a
nonce is assigned, the request is recoverable but cannot be abandoned or reused:
the signer must retry the same intent. This avoids both duplicate signing and a
terminal hole in an account's strict nonce sequence.

Nonce allocation is keyed only by network-bound account, not by tenant or input
channel, because the ledger nonce namespace is account-wide. One LMDB write
transaction stores the assigned nonce, its request owner, the next local nonce
floor and the updated reservation. The current finalized ledger nonce can raise
that floor but cannot replace an already assigned nonce.

The canonical signed envelope is persisted before publication to a mempool or
peer. Retries therefore resend the same bounded bytes and operation ID. An
unknown broadcast result becomes `Indeterminate`; it does not authorize a new
operation. Timeouts are operational signals only and never release a nonce.

Finalization is accepted only when the receipt index returns the operation's
verified checkpoint and receipt. The core checks operation ID, account, nonce and
ledger idempotency key before making the transition durable. Reconciliation is
idempotent across retries and restarts.

LMDB keeps primary request rows plus account/nonce and operation-ID ownership
indexes. Every mutation is one transaction. Reopen checks checksums, canonical
envelopes, state invariants and all secondary indexes before serving requests.
The environment uses LMDB locking and full durability; unsafe memory mapping is
confined to the documented adapter boundary.

## Trust boundaries

- Authentication and tenant/account authorization happen before reservation.
- HTTP, wallet, POS and feature-phone adapters must map their authenticated input
  into the canonical request without changing its semantics.
- Persisting an envelope proves identity and recoverability, not signature
  validity; normal transaction ingress still performs authoritative signature
  and policy verification before admission.
- Only the verified finalized-receipt index can prove payment completion. A
  mempool acknowledgement or transport response is not finality.

## Consequences

Lost HTTP, POS, SMS and USSD responses no longer require callers to guess whether
to create a new payment. The durable record can return the assigned nonce, exact
envelope, pending state or finalized receipt.

Nonce reservations intentionally favor safety. A request stuck after nonce
allocation requires signing/reconciliation recovery and cannot be silently
expired. Operational tooling must expose and alert on these records.

Background scanning, retention checkpoints, public API schemas, multi-process
gateway service hosting and a production HSM signer adapter remain separate
work. The current implementation supplies the invariant-preserving foundation,
not a public merchant HTTP API.
