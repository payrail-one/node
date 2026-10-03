# ADR-0033: Bounded payment finality reconciliation

## Status

Accepted for the Rust reference implementation.

## Context

The journal-first gateway can recover an individual request when its identifier
is already known. A production gateway must also recover work after a crash
without loading or scanning every historical request. A full journal scan would
grow with completed payment history, delay recent requests and create an
unbounded restart workload.

Finality lookup can fail for one operation while other operations remain
readable. A background worker must therefore isolate item failures instead of
aborting an entire page and starving later requests.

## Decision

Maintain a derived `reconciliation_queue` LMDB database keyed by the canonical
96-byte payment request identifier. It contains exactly reservations in
`SubmissionPrepared`, `Published` or `Indeterminate` state.

- preparing the signed envelope inserts the queue entry in the same LMDB write
  transaction as the journal record and operation-owner index;
- publication-state changes retain the entry;
- finalization removes it in the same transaction as the finalized journal
  record;
- pre-submission reservations, nonce assignments and rejections never enter the
  queue;
- reopening validates that journal states and queue membership are identical.

An atomic metadata marker supports upgrading stores created before this index.
When the marker is absent, the adapter decodes and validates existing records,
rebuilds the queue and publishes the marker in one transaction. Once the marker
exists, unexpected missing or extra queue entries are treated as corruption,
not silently repaired.

The store exposes exclusive-cursor pagination with a non-zero maximum page size
of 1,024. It reads one lookahead row so `next_cursor` is present only when more
work exists. A cursor may safely refer to a row removed by an earlier
finalization because LMDB range seeking resumes strictly after its byte key.

`PaymentFinalityReconciler::reconcile_batch` checks the payment and receipt
network bindings once, scans one bounded page, and retains payment or receipt
errors on the affected item. Other items in the page continue independently.
The caller advances `next_cursor` until it is absent, then starts a later sweep
from the beginning so entries that remained pending are reconsidered.

## Security and operational boundaries

- The queue is a derived recovery index; finalized receipt evidence remains the
  authority for completion.
- A missing receipt leaves the payment pending and never causes replacement
  signing or nonce reuse.
- The cursor is sweep-local. Persisting it across an unbounded interval without
  periodically returning to the beginning could starve earlier pending rows.
- ADR-0034 adds durable due-time scheduling, fenced same-host leases and
  aggregate telemetry. Permanent-error classification and dead-letter policy
  remain open.
- Journal and receipt retention, distributed multi-host coordination and
  disaster-recovery orchestration remain separate production decisions.

## Consequences

Restart work now scales with unresolved submissions rather than total payment
history. The integration test proves bounded multi-page traversal, continued
progress after an injected receipt-store error, exact residual queue state and
successful completion after closing and reopening both LMDB stores.
ADR-0034 extends the queue value and adds a due-time secondary index without
changing these journal-membership invariants.
