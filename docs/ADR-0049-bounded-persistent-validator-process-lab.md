# ADR-0049: Bounded persistent validator-process payment laboratory

## Status

Accepted for the Rust reference implementation. Its fresh-connection boundary
is superseded by ADR-0050.

## Context

ADR-0045 intentionally restarted every validator between finalized blocks to
prove durable recovery. That is necessary recovery evidence, but it does not
exercise the normal operating path of a validator service that processes many
blocks without restarting. Recreating seven operating-system processes for
every block also prevents a meaningful separation between process startup,
TLS-session setup and payment finality.

The first-quorum network profile in ADR-0048 still used the restart lifecycle.
Before a remote sustained benchmark, the same signed-operation, finality and
LMDB path needs a bounded long-lived process mode without removing the existing
restart-recovery scenario.

## Decision

The payment process harness now supports two explicit lifecycles over one
authoritative execution path:

1. `RestartPerBlock` remains the compatibility and recovery mode. Every block
   starts seven processes with one allowed session, finalizes and commits, then
   stops the processes before the next block.
2. `Persistent` starts the seven validator processes once. Each process binds
   one mTLS listener, loads and verifies its network configuration, validator
   set and consensus key once, and accepts a bounded number of connections.
3. The session limit is mandatory, non-zero and capped at 64. The laboratory
   never creates an unbounded daemon accidentally.
4. Every round reloads the validator's current finalized checkpoint and
   canonical ledger state from its authoritative LMDB, independently
   executes the proposed signed-operation block, reserves the vote in the
   durable anti-double-sign journal, verifies the returned finality certificate
   and atomically commits before acknowledging.
5. A malformed, cancelled or disconnected session is rejected and closed
   without terminating the bounded service loop. Before every subsequent block,
   the coordinator checks that every persistent child process is still alive.
   A process exit fails the run closed.
6. Restart and persistent modes share block preparation, first-valid-quorum
   collection, individual vote verification, certificate construction,
   validator commit acknowledgement, coordinator LMDB publication, receipt
   recovery and final audit code.
7. The report identifies persistent-process mode, total process starts and
   restart count. The executable output also identifies debug or release build.

The diagnostic command is:

```text
cargo run -p validator-process-harness -- \
  --payment-finality-persistent-degraded-national
```

It runs 16 blocks of 64 real signed transfers through seven processes under the
ADR-0048 2/50/125 ms degraded-national profile. This is 1,024 transfers and 80
selected-validator durable commit acknowledgements with zero process restarts.
The first recorded release diagnostic and its reproducibility limits are in
`PERSISTENT_PAYMENT_LAB.md`.

## Failure and recovery semantics

- Session failure cannot mutate the coordinator ledger because publication
  still requires a verified weighted certificate and every selected validator's
  exact checkpoint acknowledgement.
- A validator that voted but did not receive finality may retain its signing
  reservation and remain at the previous checkpoint. It cannot replace a
  committed quorum member until authenticated catch-up succeeds.
- Persistent-process liveness is checked before the next block. An early clean
  exit and a crash are both treated as `ChildExited`.
- The restart-per-block integration scenario remains mandatory. Persistent
  success is not substituted for restart recovery evidence.

## Security and measurement boundaries

- ADR-0050 adds bounded persistent mTLS connection reuse, fail-closed
  backpressure and a whole-round deadline; ADR-0051 adds fenced peer health,
  retry backoff and security quarantine. Asynchronous multiplexing and
  multi-host scheduling remain separate work.
- Configuration and consensus key material remain in process memory for the
  bounded run. Production validators require HSM or remote-signer integration,
  hardened secret lifecycle and process supervision.
- The service loop exposes low-cardinality health and rejection counts, but
  production metric export and durable security-incident evidence remain open.
- Sixteen local blocks are a regression and trend diagnostic, not sustained
  throughput, a soak test or a publishable latency percentile. Multi-host
  traffic shaping and independent reproduction remain open.

## Consequences

The repository can now compare restart recovery with a normal long-lived
validator-process path while preserving identical monetary, cryptographic and
durability checks. ADR-0050 subsequently removes steady-state per-block TLS
handshakes without representing local fault injection as production networking.
