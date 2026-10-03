# ADR-0050: Bounded persistent validator connections and round deadlines

## Status

Accepted for the Rust reference implementation and remote-benchmark preparation.
Peer health and retry classification are extended by ADR-0051.

## Context

ADR-0049 removed per-block operating-system process startup from the normal
laboratory path, but every block still created a new TCP and mutually
authenticated TLS connection. That hid neither handshake cost nor connection
churn and did not exercise the lifecycle needed by a low-latency payment
service. A reusable channel must never allow two concurrent requests to corrupt
one framed stream, and a timed-out or partially consumed stream must never be
returned to service.

The first-quorum path also had individual socket timeouts but no single budget
covering proposal delivery, vote collection, proof construction, finality
delivery and commit acknowledgement.

## Decision

1. Persistent-process mode owns a bounded pool with exactly one slot for each
   configured validator. A slot permits at most one in-flight round.
2. Checkout uses fail-closed, non-blocking admission. Concurrent use of the same
   validator slot returns `Backpressure`; it does not queue an unbounded number
   of requests or open an extra connection.
3. A healthy connection returns to its slot only after the validator has
   verified the finality proof, atomically committed its LMDB state and returned
   an acknowledgement for the exact checkpoint. Any cancellation, timeout,
   framing error, identity mismatch, invalid signature, invalid acknowledgement
   or incomplete round discards the connection.
4. An empty slot lazily establishes a fresh mTLS connection with the existing
   pinned certificate fingerprint and ALPN checks. This provides bounded
   reconnect after a discarded channel without weakening peer admission.
5. One accepted validator connection may process multiple sequential rounds.
   Before every round the validator independently reloads its authoritative
   finalized checkpoint and canonical state from LMDB, executes the signed
   operations, journal-first reserves its vote, verifies finality and commits
   before acknowledging.
6. Every payment-finality round has one six-second monotonic deadline. Link
   injection, connection establishment, framing I/O, vote collection and
   validator commit acknowledgement all consume the same budget. Expiry returns
   `RequestDeadlineExceeded` and prevents coordinator publication.
7. The report exposes whether pooling is enabled, opened, reused and discarded
   connections, backpressure rejections and peak concurrent leases. Completion
   additionally requires zero outstanding leases.
8. Restart-per-block recovery remains an independent path and deliberately uses
   ephemeral connections. Connection reuse cannot replace restart and recovery
   evidence.

## Failure and recovery semantics

- A channel is reusable only at a protocol boundary proven by an exact durable
  commit acknowledgement. There is no attempt to resynchronize a partially
  consumed byte stream.
- A cancelled non-quorum request drops its lease. Its validator may have a
  durable signing reservation but cannot be treated as committed without the
  finality exchange and acknowledgement.
- Losing a pooled channel does not change monetary state. A later round may
  reconnect, but the validator's LMDB checkpoint and signing journal remain
  authoritative.
- Deadline expiry or backpressure can reduce responders. Finality proceeds only
  if the remaining independently verified weight still reaches quorum.

## Security and measurement boundaries

- This is a single-coordinator, one-in-flight-round-per-peer laboratory pool,
  not a production asynchronous P2P transport or consensus implementation.
- The six-second bound is a harness safety deadline, not the retail SLO. The
  product objectives remain API acknowledgement p95 at 250 ms or less and
  durable finality p95 at two seconds or less.
- ADR-0051 supplies runtime-independent peer health, exponential backoff and
  security quarantine. Production async scheduling, durable incident evidence,
  admission fairness, multi-host traffic shaping, HSM/remote-signer latency and
  sustained concurrent load tests remain open.
- Application-layer deterministic delay and a short local run are not evidence
  of national-network or card-network capacity.

## Consequences

The persistent laboratory now measures steady-state authenticated channel reuse
instead of paying a TLS handshake for every finalized block. The same change
makes failure semantics stricter: only completely acknowledged channels are
recycled, overload is explicit, and all round phases share one bounded time
budget. Remote multi-host and sustained-load gates remain open.
