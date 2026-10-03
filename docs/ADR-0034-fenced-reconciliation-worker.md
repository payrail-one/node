# ADR-0034: Fenced reconciliation worker and durable retry schedule

## Status

Accepted for the Rust reference implementation.

## Context

ADR-0033 introduced a bounded queue of unresolved signed payments. A production
service still needs to coordinate multiple worker processes, avoid retry storms,
survive process restart and expose actionable operational signals.

A time-only lease is insufficient. A paused owner can resume after expiry and
write concurrently with its replacement. Retry state held only in memory also
causes every restart to retry all pending operations immediately.

## Decision

Define storage-independent work-coordination and service orchestration ports in
`payment-reconciliation-work-core` and
`payment-reconciliation-service-core`.

The LMDB payment journal now maintains two derived indexes in the same
environment:

- `reconciliation_queue`, keyed by payment request ID, stores consecutive error
  count and the next permitted attempt time;
- `reconciliation_due`, keyed by big-endian attempt time plus request ID,
  provides deterministic earliest-due scans without walking deferred work.

Creating a signed submission publishes both rows in the journal transaction.
Rescheduling atomically removes the old due key and writes the new schedule and
due key. Finalization atomically removes both rows. Reopen validation proves a
one-to-one relationship between reconcilable journal records, schedules and due
keys. The prior queue format is rebuilt once under an atomic schema marker.

Worker ownership uses an LMDB metadata lease with a monotonically increasing
fencing token. Acquisition is allowed only after the previous lease expires.
Every due scan, renewal, deferral and acknowledgement checks the exact owner,
token and expiry. A stale owner therefore cannot mutate scheduling state after
takeover. The token counter is never reset when a lease is released.

Each service cycle is bounded by the configured page limit. The worker renews
its lease before every item, distinguishes a normal missing receipt from an
infrastructure/domain error, and applies:

- a fixed positive polling delay for pending finality;
- capped integer exponential backoff for errors;
- an explicit consecutive-failure alert threshold;
- aggregate cycle telemetry without payment identifiers or error text.

Successful cycles release the lease immediately. If clock or coordination
infrastructure fails, the worker does not attempt an unsafe unconditional
release; the lease expires and another fenced owner can recover.

## Security and operational boundaries

- Finalized indexed receipt evidence remains the only authority that completes
  a payment. Scheduling never creates or replaces an operation.
- The clock must not move backwards within a cycle. Regression fails closed.
- `WorkerId` is a non-zero process-instance identifier and must be unique for
  each boot; it is not a long-lived host identity or secret.
- LMDB safely coordinates processes on one host. It must not be placed on a
  network filesystem. Multi-host HA must implement the same fenced port using a
  suitable distributed transactional store.
- Generic errors are retried and alerted, but permanent-error classification,
  protected diagnostic storage and dead-letter governance remain open.
- The core runs one cycle; process supervision, sleep/jitter, shutdown,
  readiness and metrics export belong to the future service host.

## Consequences

Pending and failed payment recovery is now due-time ordered, bounded and
restart-safe. Tests prove lease exclusion, exact-expiry takeover, monotonically
increasing fencing tokens, rejection of stale renew/defer calls, durable pending
and error schedules, no early retry, and finalization after full LMDB reopen.
