# ADR-0051: Fenced peer health, retry backoff and security quarantine

## Status

Accepted for the Rust reference implementation and production peer-manager
boundary.

## Context

ADR-0050 introduced bounded persistent validator connections. A failed channel
was discarded safely, but the next block immediately tried the same peer again.
That behavior can create reconnect storms, waste file descriptors and CPU, and
amplify a remote or network failure. Treating an authentication or protocol
violation as an ordinary transient network error is also unsafe: automatic
retry would repeatedly trust a peer that has contradicted its admitted identity
or consensus key.

Health policy must remain independent from a particular async runtime, socket
implementation and metrics vendor. Completion from an old connection must not
change the health of a replacement peer configuration.

## Decision

1. `peer-health-core` owns the framework-independent peer lifecycle. It accepts
   caller-supplied monotonic milliseconds and has no networking or runtime
   dependency.
2. Every attempt is single-flight and fenced by both configuration generation
   and monotonic sequence. Busy, stale-generation and stale-sequence completion
   cannot change peer health.
3. Transient `Connect`, `Timeout` and `Transport` failures move a peer from
   `Healthy` to `Degraded`, then to capped exponential `BackingOff` after a
   configured threshold. Attempts before `retry_at_ms` fail explicitly.
4. `Authentication` and `ProtocolViolation` failures immediately move a peer to
   `Quarantined`. Time alone never clears quarantine. A new authenticated
   configuration generation must construct new health state.
5. Recovery can require multiple consecutive successful attempts before the
   peer becomes `Healthy` again. One successful probe is not universally
   assumed to prove recovery.
6. Time regression, retry-time overflow and attempt-sequence exhaustion fail
   closed without replacing the active attempt or partially changing state.
7. Telemetry is low-cardinality and contains counts and state only: started and
   successful attempts, transient and security failures, busy/backoff/quarantine
   rejections, consecutive outcomes and the last failure class. It contains no
   payment or account identifiers.
8. The validator connection pool uses the same state machine. For the bounded
   deterministic laboratory it enters a 60-second backoff after two transient
   failures. This laboratory policy is not a production default.
9. A pooled stream is marked successful only after exact durable checkpoint
   acknowledgement. Invalid response encoding or acknowledgement is a protocol
   violation; node mismatch or invalid consensus signature is an authentication
   failure; cancelled and lost transport paths are transient.

## Failure and recovery semantics

- A backoff or quarantined peer contributes no vote. Finality still requires
  independently verified quorum weight from available peers.
- A late result carrying an old attempt fence returns `StaleAttempt` and cannot
  heal, degrade or quarantine the current generation.
- A health-state error never changes ledger, signing-journal or finality state.
- Rebuilding health after configuration change does not reuse the previous TCP
  or TLS stream; normal membership, certificate and protocol admission still
  applies.

## Security and operational boundaries

- Health state is currently process-local. Production supervision must persist
  or externally alert on security quarantine so a restart cannot silently erase
  incident evidence.
- The state machine is ready for an asynchronous peer manager, but the current
  harness still uses bounded threads and one coordinator. Multi-host connection
  ownership, jittered reconnect scheduling and graceful draining remain open.
- Operators must tune thresholds by deployment and failure domain. The
  deterministic 60-second laboratory backoff is intentionally unsuitable as a
  universal policy.

## Consequences

Transient outages no longer force unbounded immediate reconnect, while
cryptographic or protocol contradictions no longer enter an automatic retry
loop. The same domain policy can be reused by a future async P2P service without
coupling consensus safety to its runtime.
