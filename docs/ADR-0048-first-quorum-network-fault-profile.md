# ADR-0048: First-quorum payment path and deterministic network fault profile

## Status

Accepted for the Rust reference implementation and benchmark preparation;
extended to bounded long-lived validator processes by ADR-0049.

## Context

The multi-process payment laboratory already joined independently executed
signed operations, weighted finality and atomic normalized LMDB publication.
Its loopback timing did not exercise regional delay or directed loss. It also
collected every successful vote before continuing even though the certificate
only required the first weighted quorum. A slow non-quorum validator must not
delay a retail payment after enough authenticated votes are available.

The roadmap requires p95 and p99 finality evidence under unavailable validators
and 100–300 ms regional links. A deterministic single-host fault profile is an
intermediate verification step; it must not be represented as a remote-host or
production SLO result.

## Decision

The payment process harness now provides a bounded `PaymentNetworkProfile` at
the coordinator transport boundary:

1. Every validator path has four independently directed conditions: proposal,
   vote return, finality commit and commit acknowledgement.
2. A condition specifies deterministic one-way delay and delivery or loss. The
   public custom-profile constructor rejects delays above one second so a test
   configuration cannot accidentally create an unbounded laboratory run.
3. The degraded-national preset uses a 3/2/2 layout: 2 ms one-way delay for
   three validators, 50 ms for two and 125 ms for two. One validator is
   unavailable and one additional validator's return-vote path is blocked. The
   five remaining validators are exactly the equal-weight quorum.
4. Delay and loss surround real mutually authenticated TLS I/O, real validator
   runtime execution and real Ed25519 votes. Fault injection does not replace
   signature, peer-identity, checkpoint or payload verification.
5. Vote tasks report as they complete. The coordinator checks peer identity and
   each Ed25519 signature against the exact finality message before adding that
   validator's weight. It advances as soon as the first valid weighted quorum is
   present, cancels remaining injected waits, builds and verifies a certificate
   containing only that quorum, and does not wait for a slower non-quorum
   response.
6. The verified certificate is returned to every selected signer. Every
   selected validator must atomically commit its normalized LMDB state and
   acknowledge the exact checkpoint before the coordinator publishes the same
   transition. A missing selected acknowledgement fails the round closed.
7. Each block records nearest-rank p50, p95, p99 and maximum vote-quorum and
   durable-finality latency. Validator-commit and coordinator-commit phases are
   also reported separately. Process startup, fixture/bootstrap work, receipt
   indexing and child-process teardown are outside durable-finality timing.

The executable scenario is:

```text
cargo run -p validator-process-harness -- \
  --payment-finality-degraded-national
```

## Failure and recovery semantics

- A validator may durably reserve a vote before its simulated return path is
  lost. Cancellation does not erase that anti-double-sign record.
- Validators outside the selected quorum do not receive the certificate and
  remain at their previous finalized checkpoint. They require the authenticated
  catch-up path before replacing a committed quorum member in a later round.
- A selected validator that cannot acknowledge its durable commit aborts the
  laboratory round. The coordinator never treats a valid certificate alone as
  proof that the selected replica set published state.
- Fewer than five reachable equal-weight validators still produce
  `InsufficientQuorum` and no coordinator ledger publication.

## Security and measurement boundaries

- This is deterministic application-layer fault injection around loopback
  sockets. It does not model bandwidth contention, queueing, kernel traffic
  control, Internet jitter, packet reordering, clock skew or independent hosts.
- The parent remains a central laboratory proposer. Production block
  propagation, view changes, fork choice and peer-to-peer consensus remain out
  of scope.
- Four default blocks are enough for a regression scenario, not for publishable
  percentiles, sustained throughput or an SLO claim. Remote multi-host runs,
  long duration, realistic account mixes, HSM latency and independent
  reproduction remain required.
- The first-quorum path cancels bounded injected waits. Production networking
  needs an asynchronous request lifecycle with explicit deadlines, connection
  reuse, backpressure and observability.

## Consequences

The repository now exercises the payment fast path under repeatable latency,
one unavailable validator and asymmetric message loss without weakening
certificate or durable-state verification. Latency output is split into
merchant-relevant durable finality and its constituent phases. The result is a
stronger benchmark precursor, not evidence that the production network already
meets Visa/Mastercard-scale latency or capacity.
