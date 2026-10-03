# ADR-0047: Supervised receipt-index worker process boundary

## Status

Accepted for the Rust reference implementation and operational-harness work.

## Context

ADR-0046 proved that a derived receipt index can lag behind authoritative ledger
finality and deterministically recover from retained payloads. Its catch-up was
still invoked in the coordinator process, so the laboratory did not exercise
process isolation, independent configuration verification, worker exit failure,
bounded completion time or restart-visible LMDB state.

The receipt worker must not inherit consensus signing authority or become part
of the atomic monetary commit. At the same time, the supervisor must not accept
a successful-looking result merely because a child process was started.

## Decision

Receipt catch-up now runs as a separately spawned OS process:

1. The supervisor supplies an explicit network ID plus paths to the signed
   network configuration, authoritative ledger LMDB, derived receipt LMDB and a
   unique result file. No consensus seed is passed as an argument or loaded by
   the worker.
2. The worker independently verifies the signed network configuration against
   the pinned configuration public key before opening either database.
3. `LmdbTailStateStore` validates authoritative history on open. The worker then
   executes `receipt-rebuild-core` with the configuration-bound state commitment
   policy and commits derived receipt batches through `LmdbReceiptIndex`.
4. On success the worker writes a fixed-size, domain-separated, checksummed
   report containing the recovery heights, replay counts, newly indexed counts
   and final operation cursor. The file is created once with private permissions
   and fsynced together with its parent directory.
5. The supervisor polls the child with a ten-second laboratory timeout. A
   timeout or status-poll error kills and reaps the process; a non-zero exit,
   missing, oversized, malformed or corrupt report fails the scenario.
6. Only after successful exit and report verification does the supervisor use
   the result. It independently reopens the receipt index and cross-checks every
   operation-index, operation-ID, account/nonce and correlation lookup.

The multi-block scenario launches two workers: one after the first finalized
block and one after the deliberate three-block indexing outage. Core finality
timing remains separate from worker startup and receipt-index work.

## Failure semantics

- Worker failure cannot change or roll back the authoritative ledger cursor.
- A partially completed receipt transaction remains invisible because the index
  publishes each block and cursor in one LMDB transaction.
- If the worker commits derived data but fails before reporting, a later rebuild
  can verify the existing block idempotently; production supervision must retry
  from the durable receipt cursor rather than from an in-memory assumption.
- The report checksum detects accidental/truncated local output. It is not a
  remote authenticity mechanism; the laboratory workspace is process-owned.

## Security and operational boundaries

- This is a bounded one-shot worker, not yet a long-running service with leases,
  exponential backoff, health endpoints, metrics or deployment orchestration.
- File paths are local laboratory capabilities. Production services require
  dedicated OS identities, least-privilege directory access and secret-manager
  delivery of trust configuration.
- The ten-second timeout is a local harness safety bound, not a production SLO.
- Multi-host object storage, online LMDB map growth and long-history rebuild
  checkpoints remain future operational work.

## Consequences

Receipt availability is now exercised behind a real process boundary without
expanding validator authority or the monetary commit. The harness can detect
untrusted configuration, non-zero worker exit, hangs, malformed completion
evidence and restart divergence while preserving ledger liveness.
