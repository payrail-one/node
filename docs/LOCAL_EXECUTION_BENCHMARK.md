# Preliminary local payment execution benchmark

Date: 2026-09-20

This result validates the benchmark harness and provides an engineering
baseline. It is not a consensus, network-finality or production capacity claim.

The measurement predates ADR-0036. The canonical transfer envelope is now 322
bytes because it carries a signature-bound consensus expiry height. The figures
below remain historical optimization evidence and must be rerun before they are
used as a current capacity result.

## Workload

```text
50,000 measured operations
5,000 warm-up operations
single sender / hot account
canonical authorization
Ed25519 signing
314-byte pre-ADR-0036 binary envelope encode and decode
strict Ed25519 verification
atomic ledger transfer and receipt commit
post-run nonce and balance reconciliation
```

Command:

```sh
cargo run --release --manifest-path platform/Cargo.toml \
  --package payment-benchmark -- --operations 50000 --warmup 5000
```

## Result

| Metric | Result |
| --- | ---: |
| Throughput | 11,954.04 transfers/s |
| Total measured time | 4.182684750 s |
| p50 operation latency | 81.708 µs |
| p95 operation latency | 93.542 µs |
| p99 operation latency | 112.208 µs |
| Signed envelope | 314 bytes |

## Environment

- Apple M1 Max, 10 logical CPUs;
- macOS 26.6.2, Darwin 25.6.0 arm64;
- rustc 1.94.1, optimized release profile;
- repository HEAD `37e272098ba9fc88335ee1a8c94094f9b0cc7407`, with the new `platform/`
  work still uncommitted.

Because the implementation is not yet bound to a commit, this result is
preliminary and must not be published as a reproducible figure. The historical
BSI Temtum result used different x86 hardware and an unspecified transaction
path, so the numbers are not a like-for-like performance comparison.

## Next measurement gate

The publishable benchmark must use signed source state and include seven
validators, three failure domains, 100–300 ms network delay, deterministic
finality percentiles, node failure, live catch-up, clean bootstrap and verified
state-root application. API acknowledgement and committed/final status are
reported separately.
