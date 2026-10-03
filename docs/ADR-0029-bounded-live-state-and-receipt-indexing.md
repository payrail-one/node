# ADR-0029: Bounded live ledger state and receipt indexing

## Status

Accepted for the reference ledger, runtime and LMDB adapter. This decision
supersedes the receipt-in-snapshot requirements in ADR-0003, ADR-0005,
ADR-0019 and ADR-0024. A production receipt index and archival retention
policy remain required before a persistent public testnet.

## Context

Receipts and client idempotency mappings are append-only history, not data
needed to decide whether the next payment is valid. Keeping every historical
receipt and operation identifier in the live consensus snapshot made canonical
state serialization, authenticated-state preparation, state sync and restart
cost grow with total transaction history. A 10,000-payment local diagnostic
showed that this growth materially reduced throughput even though balances and
account count were constant.

Removing history from live state must not permit replay, reuse a receipt
sequence, lose finalized payment evidence, or let a gateway turn a retry into a
second payment.

## Decision

- The sender's exact next nonce is the consensus replay authority. A replayed
  signed envelope fails with `NonceMismatch` before any state mutation.
- Live ledger state contains assets, non-zero balances, non-zero account
  nonces, non-default account policies and one `next_operation_index` counter.
  It does not contain historical receipts, operation IDs or idempotency keys.
- Every successful payment consumes exactly one sender nonce and exactly one
  operation index in the same atomic transition. Snapshot restoration requires
  the checked sum of all stored account nonces to equal
  `next_operation_index`; zero nonce rows and arithmetic overflow fail closed.
- Runtime execution still returns one deterministic `OperationReceipt` per
  included operation. Receipts are block-local execution results and are not
  copied into the next consensus state image.
- `IdempotencyKey` remains signed payment metadata for wallet, merchant,
  SMS/USSD and provider correlation. It is not a permanent consensus-unique
  key. Reusing it with a fresh valid nonce is therefore permitted by the
  ledger.
- Gateways must persist an idempotency mapping scoped to the authenticated
  client/account and canonical request. An exact retry returns the original
  operation ID and status. A different request under the same active key fails
  closed. A retry must not be re-signed with a new nonce merely because an API
  response was lost.
- LMDB retains the complete canonical payload for every finalized post-base
  block in a separate database. The payload hash is committed by the immutable
  block record and rechecked on lookup and every reopen.
- One LMDB transaction publishes the normalized live-state delta,
  authenticated-tree update, bounded complete state image, block record, full
  payload and finalized cursor. `MapFull`, a root mismatch or any conflicting
  retry publishes none of them.
- A receipt indexer derives durable query records from finalized payloads and
  deterministic execution results. Re-indexing from a trusted base snapshot
  and the contiguous finalized payload archive must reproduce the same
  operation IDs, nonces, indices and receipt kinds.
- Receipt history before a state-sync base height requires an archive/index
  checkpoint supplied and verified separately; it cannot be reconstructed from
  the current state snapshot alone.
- The canonical state domain is `ledger.state.v2`, and the authenticated
  operation namespace now contains one operation-sequence row. Version-one
  receipt-bearing state is never silently reinterpreted; migration requires an
  explicit converter or new genesis.

## Security properties

- Dropping receipt history cannot make a previously finalized signed payment
  executable again because account nonce state remains consensus-authenticated.
- Sequence exhaustion and nonce increment overflow are checked before balance,
  nonce, sequence or event mutation.
- A full payload cannot be substituted without violating the block record's
  payload hash. Missing or corrupt payload history makes LMDB reopen fail
  closed.
- `OperationId` remains a domain-separated SHA-256 commitment to canonical
  payment intent. Indexers must compare canonical content on an impossible-in-
  practice identifier collision rather than overwrite an existing record.
- API idempotency availability is not consensus safety. If the gateway mapping
  is unavailable, the service must return an indeterminate/retry-later result
  or locate the finalized operation; it must not allocate a new nonce and pay
  again.

## Evidence

Tests prove exact-envelope replay rejection, deliberate idempotency-key reuse
with a fresh nonce, snapshot sequence validation, overflow atomicity, complete
payload survival across restart, exact-retry equality and map-full rollback of
state, block, payload and cursor together. Runtime tests continue to prove
receipt ordering and whole-block failure atomicity.

The local pipeline benchmark now reports a constant 534-byte final canonical
state for both 1,000 and 10,000 sequential payments in the fixed-account
fixture, with two retained complete state images. Three comparable 1,000-payment
runs produced 14,276.63, 11,644.47 and 11,422.89 transfers/s; the median was
**11,644.47 transfers/s**, 22.20% above the previous receipt-bearing-state
median. A separate 10,000-payment run sustained **14,528.65 transfers/s** over
20 blocks, retained all 20 finalized payloads and ended with the same 534-byte
state. Performance figures remain local engineering diagnostics and exclude
validator voting, WAN, HSM, API and merchant-device latency.

## Consequences and open work

Consensus-state and state-sync cost now scale with live accounts and assets,
not total payment history. Full finalized payload history still grows and must
be capacity-planned, archived and eventually pruned under a policy coordinated
with receipt-index checkpoints, audit requirements and proof availability.

ADR-0030 now supplies the independent finalized receipt schema, atomic block
consumer boundary and deterministic rebuild path. Production finality
subscription, archive checkpoints, retention/privacy controls and gateway
pre-admission idempotency reservation remain open. SMS/USSD, merchant and
exchange APIs must use those layers before they can claim safe retry behavior.
