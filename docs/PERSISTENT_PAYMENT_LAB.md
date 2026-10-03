# Persistent payment-process laboratory result

## Scope

This is a local engineering diagnostic for the bounded persistent-validator
path defined by ADR-0049. It is not a production benchmark, an SLO result or a
Visa/Mastercard capacity comparison.

The measured command was:

```text
cargo run --release -q -p validator-process-harness -- \
  --payment-finality-persistent-degraded-national
```

## Environment

- Date: 2026-09-21
- Host: Apple M1 Max, arm64, 64 GiB RAM
- OS: Darwin 25.6.0
- Rust: `rustc 1.94.1 (e408947bf 2026-03-25)`
- Cargo: `cargo 1.94.1 (29ea6fb6a 2026-03-24)`
- Build: release
- Repository HEAD: `37e272098ba9fc88335ee1a8c94094f9b0cc7407`
- Source state: dirty working tree with 15 reported paths; the new platform tree
  is not yet represented by the repository commit, so this result is not
  independently source-reproducible or publishable.

The same source state passed `scripts/quality.sh`, including source-size checks,
format verification, Clippy with warnings denied and all workspace tests.

## Workload and fault profile

- seven persistent validator OS processes;
- zero validator process restarts;
- 16 sequential blocks;
- 64 real Ed25519-signed transfers per block;
- 1,024 finalized transfers and 334,144 encoded payload bytes;
- equal-weight 5/7 finality and 80 durable validator commit acknowledgements;
- 3/2/2 link profile with 2/50/125 ms deterministic one-way delay;
- one unavailable validator and one additional asymmetric lost return-vote path;
- real mTLS, independent validator runtime execution, finality proof verification
  and atomic normalized LMDB commits;
- bounded persistent connection pool with one in-flight lease per validator;
- seven authenticated connection opens, 75 healthy connection reuses and two
  expected discards before the injected lost-vote peer entered backoff;
- 14 suppressed reconnect attempts, zero backpressure or quarantine rejections,
  six healthy peers, one backing-off peer and a peak of six simultaneous leases;
- receipt index paused for 15 blocks and recovered afterward from finalized
  payload history.

## Observed result

| Measurement | Result |
| --- | ---: |
| Vote quorum p50 | 124.732 ms |
| Vote quorum p95 | 355.370 ms |
| Vote quorum p99 | 355.370 ms |
| Durable finality p50 | 290.290 ms |
| Durable finality p95 | 510.878 ms |
| Durable finality p99 | 510.878 ms |
| Maximum durable finality | 510.878 ms |
| Total sequential durable-finality time | 4.882 s |
| Finalized height | 216 |
| Final operation index | 1,024 |
| Retained finalized payloads | 16 |
| Rebuilt and verified receipts | 1,024 |

All 16 blocks finalized, all 80 selected-validator commits were acknowledged,
the coordinator reopened and verified the complete sequential block archive,
and the independent receipt worker rebuilt every lookup after the simulated
15-block indexing outage.

## Interpretation and open gates

The run shows that per-block process restart is not required for correctness and
that this local release build remains below the preliminary two-second p95
national-pilot finality target under the deterministic profile. It does not
prove that target because there are only 16 samples on one host and the injected
delay does not reproduce real network queueing or bandwidth.

Before any publishable performance claim, the same workload must include:

- a committed or content-addressed source snapshot;
- independent remote hosts across failure domains;
- operating-system network shaping with delay, jitter, loss and bandwidth caps;
- production asynchronous multi-host scheduling and durable quarantine evidence;
- HSM or remote-signer latency;
- sustained duration and a 30-day soak run;
- realistic merchant/account contention and concurrent ingress;
- complete hardware, topology, error and retry reporting;
- independent reproduction.
