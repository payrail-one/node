# ADR-0030: Independent finalized receipt index and deterministic rebuild

## Status

Accepted for the reference receipt-query path. Gateway idempotency reservation,
archive checkpoints, privacy retention and production service orchestration
remain required before a public testnet.

## Context

ADR-0029 removed append-only receipts from live consensus state so payment cost
does not grow with total history. Wallets, merchants, SMS/USSD gateways,
exchanges and support systems still need durable receipt lookup by operation ID,
global sequence, sender nonce and signed client correlation data.

Putting those query indexes back into the validator's atomic consensus commit
would make payment finalization depend on non-consensus availability and add
unbounded secondary-index writes to the hot path. A derived index may lag, but
it must never invent a receipt, skip a fork/gap, silently diverge from finalized
execution or turn an exact retry into a second indexed operation.

## Decision

- `receipt-index-core` owns the sequential cursor, finalized receipt-batch
  model and storage/source ports. It validates network, height, parent,
  contiguous operation indices, within-block operation IDs and account nonces
  before any adapter mutation.
- `receipt-index-lmdb` is an independent LMDB environment. It is not opened in
  the validator's finalized-state transaction and cannot block or roll back
  consensus finality.
- The index is initialized from a verified recovery-base checkpoint and its
  validated `next_operation_index`. It intentionally makes no claim about
  receipts before that base unless a separate archive checkpoint is supplied.
- One index transaction publishes all receipts in a finalized block, the block
  digest record, operation-ID records, global-index records, account/nonce
  records, account/idempotency-key/nonce correlation records and the new index
  cursor.
- `IdempotencyKey` is not globally unique. The correlation index includes the
  account and nonce, allowing legitimate later reuse while keeping each signed
  operation unambiguous.
- Exact block retries are idempotent only when the block record, receipt digest,
  receipt bodies and every secondary index match. Conflicts, historical
  operation-ID reuse and account/nonce reuse fail before publication.
- Every open validates the checksummed cursor, complete contiguous block chain,
  receipt digest for each block, every secondary index and exact database row
  counts. Missing, dangling, extra or corrupt records fail closed.
- `receipt-rebuild-core` starts from the verified base snapshot, re-executes
  every archived canonical signed-operation payload through
  `LedgerBlockExecutor`, compares block and state commitments, verifies already
  indexed receipts and appends only missing finalized blocks.
- A rebuild finishes only when the index checkpoint and operation sequence
  equal the finalized source and the fully replayed ledger snapshot.
- Source and sink are ports. The current LMDB adapters implement them, while a
  future archive service or relational query store can replace either without
  changing replay validation.

## Security and failure properties

- Receipt lookup is finalized-only. Pending/mempool status remains a separate
  API concern and cannot be presented as settlement evidence.
- A forged or malformed archived payload fails signature, ledger or commitment
  verification before indexing.
- `MapFull` and all other LMDB failures publish neither partial receipts, block
  metadata nor cursor progress.
- A receipt-index outage affects query availability, not ledger safety or
  payment finality. Consumers must expose an explicit indexing-lag status.
- The index never stores private keys, authorization secrets or raw customer
  identity. Receipt metadata is still sensitive financial data and requires
  access control, encryption/backup policy and jurisdiction-specific retention.
- The localized LMDB memory-map unsafe boundary uses locking and full
  durability, rejects symlink/non-directory paths and never exposes mapped
  references beyond transaction lifetimes.

## Evidence

Tests cover side-effect-free cursor preparation, stale prepared updates,
network/gap/sequence rejection, within-block and historical collisions, exact
retry, all four query keys, deliberate idempotency-key reuse with a new nonce,
restart validation and fixed-map exhaustion atomicity.

The end-to-end rebuild test creates a verified ledger base, executes and
persists two real Ed25519-signed finalized payment blocks, rebuilds a fresh LMDB
receipt index, closes and reopens it, then replays the same archive and verifies
both existing blocks without writing duplicates.

## Consequences and open work

The platform now has a bounded consensus state plus a separately rebuildable
finalized receipt view. The next required layer is gateway idempotency
reservation before nonce allocation/signing, including `reserved`, `submitted`,
`finalized`, `rejected` and indeterminate recovery states. A finalized receipt
index alone cannot prevent an API gateway from signing a second nonce after a
lost response.

Production work also includes asynchronous finality subscription, lag metrics,
archive/index checkpoints, online map growth, backups, retention/erasure rules,
large-history soak tests and an authenticated merchant/support query API.
