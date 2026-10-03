# Preliminary local finalized-payment pipeline benchmark

Date: 2026-09-20

This benchmark is an engineering diagnostic, not a production throughput or
network-finality claim. The result intentionally includes more of the local hot
path than the earlier direct-ledger microbenchmark.

All recorded runs predate ADR-0036's additional eight-byte signed validity
field and expired-receipt branch. They remain historical engineering baselines,
not current capacity measurements; the same scenarios must be rerun after the
next material performance block.

## Measured path

```text
Ed25519 signing + canonical envelope
-> signature-verified bounded gossip pool admission
-> deterministic sender/nonce proposal selection
-> proposal execution and reproducibility check
-> finalized-block transition verification
-> normalized LMDB state/block/cursor transaction
-> finality-driven pool removal
-> final ledger reconciliation
```

The finality proof verifier is a local harness seam. Validator voting, WAN
latency, HSM latency, API transport and merchant-device latency are excluded and
must be measured separately.

## Command

```sh
cargo run --release -q -p payment-pipeline-benchmark -- \
  --operations 1000 --warmup 100 --block-operations 500
```

## Initial sequential-authorization result

| Metric | Result |
| --- | ---: |
| Transfers | 1,000 |
| Blocks | 2 |
| Configured transfers/block | 500 |
| Total measured time | 234.385 ms |
| Local pipeline throughput | 4,266.48 transfers/s |
| Block p50 | 116.232 ms |
| Block p95 / p99 | 118.152 ms |
| Largest payload | 159,020 bytes |
| Final canonical state | 117,517 bytes |

Measured stage totals:

| Stage | Time |
| --- | ---: |
| Sign, encode and signature-verified pool admission | 84.198 ms |
| Deterministic proposal and reproducibility execution | 91.422 ms |
| Finalized transition execution/verification | 45.269 ms |
| Normalized LMDB commit | 12.997 ms |
| Pool cleanup and in-memory cursor commit | 0.493 ms |

Environment: Apple M1 Max, macOS/Darwin 25.6.0 arm64, rustc 1.94.1,
repository HEAD `37e272098ba9fc88335ee1a8c94094f9b0cc7407` with the `platform/`
work still uncommitted.

## Finding and next gate

The result is below the provisional 5,000 finalized-transfers/s pilot target and
must not be rounded up or presented as equivalent to a card network. Profiling
must separate signing, pool admission, proposal execution, repeated validator
execution, canonical state encoding and LMDB commit.

The breakdown shows that LMDB is not the dominant cost in this run. Signing and
repeated signature verification, proposal execution and finalized transition
execution dominate. The current reference path also serializes a growing
receipt-bearing state and re-executes a proposer payload for reproducibility and
finalized transition verification. Production work therefore needs an
incremental authenticated state commitment, delta-based in-memory execution,
safe verification/proposal-result caching and parallel signature verification.
Every optimization must retain identical receipts, state roots, whole-block
atomicity and restart recovery.

## Bounded-parallel authorization result

ADR-0022 adds a non-serializable, process-local verified-operation capability
and bounded parallel signature checks. Monetary mutations still execute in
consensus order, and the completed proposal is independently decoded and
verified through the normal block path. Equivalence, error-order and worker
failure tests protect this boundary.

Three comparable optimized runs produced 6,955.30, 8,128.24 and 7,198.76
transfers/s. The median was **7,198.76 transfers/s**, a 68.73% increase over the
earlier 4,266.48 transfers/s single-run baseline. Representative median-run
stage totals were:

| Stage | Time |
| --- | ---: |
| Sign, encode and signature-verified pool admission | 94.567 ms |
| Deterministic proposal and reproducibility execution | 18.466 ms |
| Finalized transition execution/verification | 11.121 ms |
| Normalized LMDB commit | 14.192 ms |
| Pool cleanup and in-memory cursor commit | 0.558 ms |

The local path now clears the provisional 5,000-transfers/s threshold in these
runs, but that is not yet the roadmap SLO: the harness still excludes network
finality, seven-validator contention, signed consensus commit, WAN delay, HSM
latency and independent reproduction. Client signing plus one-at-a-time pool
admission is now the dominant measured stage. The next performance work is a
bounded batch-ingress boundary, followed by incremental authenticated state and
delta-based execution rather than repeatedly serializing growing receipt state.

## Atomic batch-ingress result

ADR-0023 replaces repeated local `publish_local` calls in the harness with one
bounded admission call per candidate block. The pool decodes all envelopes,
uses the shared parallel authorization implementation, returns an indexed
outcome for every input and calculates aggregate capacity before any mutation.
Peer gossip and final block verification are unchanged.

Three comparable runs produced 11,767.51, 9,935.87 and 10,022.43 transfers/s.
The median was **10,022.43 transfers/s**: 39.22% above the prior 7,198.76
parallel-runtime median and 134.91% above the initial 4,266.48 sequential run.
Median-run stage totals were:

| Stage | Time |
| --- | ---: |
| Client signing and canonical envelope encoding | 50.017 ms |
| Bounded parallel batch admission | 8.097 ms |
| Deterministic proposal and reproducibility execution | 20.149 ms |
| Finalized transition execution/verification | 9.056 ms |
| Normalized LMDB commit | 11.805 ms |
| Pool cleanup and in-memory cursor commit | 0.644 ms |

Signing in this harness is deliberately local and sequential; real clients sign
independently, while custodial signing requires separate HSM/MPC capacity tests.
The roadmap benchmark gate remains open until the same payment workload includes
real seven-validator finality, authenticated node transport, signed commit and
independent reproduction. The next local runtime bottleneck is full serialization
of growing receipt-bearing state, which requires a separately specified
incremental authenticated-state design rather than an unsafe shortcut.

## Persistent authenticated-state result

ADR-0024 and ADR-0025 add incremental Jellyfish Merkle Tree updates and persist
nodes, versioned values, stale/live-leaf indices and the authenticated cursor in
the same LMDB transaction as normalized rows, state, block and finalized cursor.
The benchmark therefore now measures the additional durable authenticated-state
work; the finalized checkpoint still uses the full-state commitment pending the
controlled root-format migration.

Three comparable runs produced 9,401.29, 10,895.79 and 8,663.39 transfers/s.
The median was **9,401.29 transfers/s**, 6.20% below the prior 10,022.43 local
batch-ingress median while still above the provisional 5,000-transfers/s local
threshold. Median-run stage totals were:

| Stage | Time |
| --- | ---: |
| Client signing and canonical envelope encoding | 47.317 ms |
| Bounded parallel batch admission | 7.864 ms |
| Deterministic proposal and reproducibility execution | 19.387 ms |
| Finalized transition execution/verification | 8.999 ms |
| Normalized rows + authenticated JMT LMDB commit | 22.182 ms |
| Pool cleanup and in-memory cursor commit | 0.610 ms |

The LMDB stage is now materially larger because every accepted block durably
retains proof-capable history. Further optimization must profile node encoding,
page locality and receipt growth without weakening single-transaction atomicity
or proof retention. This remains a local `network_finality:false` result and is
not a Visa/Mastercard-equivalent production claim.

## Authoritative authenticated-root result

ADR-0026 promotes the authenticated root to the finalized checkpoint at a
configured height. The benchmark now activates that policy at block one, so
proposal reproduction, independent finalized-transition verification and the
atomic LMDB commit all validate the root that consensus would sign. The JSON
output records `authenticated_checkpoint_root:true`.

Three comparable runs produced 7,447.75, 7,700.10 and 7,653.81 transfers/s.
The median was **7,653.81 transfers/s**, 18.59% below the previous 9,401.29
sidecar-root median while remaining above the provisional 5,000-transfers/s
local threshold. Median-run stage totals were:

| Stage | Time |
| --- | ---: |
| Client signing and canonical envelope encoding | 47.051 ms |
| Bounded parallel batch admission | 7.881 ms |
| Deterministic proposal and reproducibility execution | 29.626 ms |
| Finalized transition execution/verification | 16.536 ms |
| Normalized rows + authenticated JMT LMDB commit | 28.977 ms |
| Pool cleanup and in-memory cursor commit | 0.571 ms |

The added cost is expected at this correctness stage: the stateless runtime
rebuilds the authenticated view when reproducing both the proposal and the
finalized transition, while LMDB independently applies and checks the
incremental update. The next performance step is a deterministic incremental
execution view with cross-node equivalence tests. Network propagation, real
seven-validator finality and merchant-observed end-to-end latency are still not
part of this measurement.

## Two-phase incremental authenticated-root result

ADR-0028 replaces repeated complete-tree rebuilds with an opaque two-phase
authenticated update. Proposal execution prepares a normalized row delta and
retains it until finality. Tail verification checks finality before returning
that update, and the persistent LMDB tree independently prepares and checks its
delta in the atomic block transaction. The LMDB path no longer rebuilds the
complete tree before performing the same incremental root check. JSON output
records `incremental_runtime_root:true`.

Three comparable runs produced 9,250.28, 10,759.95 and 9,528.65 transfers/s.
The median was **9,528.65 transfers/s**, 24.49% above the prior 7,653.81
authoritative-root median. Representative median-run stage totals were:

| Stage | Time |
| --- | ---: |
| Client signing and canonical envelope encoding | 49.766 ms |
| Bounded parallel batch admission | 8.346 ms |
| Deterministic proposal plus incremental root preparation | 24.167 ms |
| Cached transition/finality verification | 0.001 ms |
| Normalized rows + authenticated JMT LMDB commit | 22.043 ms |
| Pool cleanup and in-memory cursor commit | 0.616 ms |

The representative two-block run had 48.840 ms p50 and 56.106 ms p95/p99
local block latency. A separate 10,000-transfer diagnostic reached 6,939.39
transfers/s over 20 blocks as receipt-bearing state grew, so state growth,
pruning and soak behavior remain explicit open work. Neither result includes
network voting/finality, WAN, HSM, API or merchant-device latency and neither is
a Visa/Mastercard-equivalent production claim.

## Bounded live-state and finalized-payload result

ADR-0029 removes append-only receipts and operation identifiers from the live
consensus snapshot. Sender nonce remains the authoritative replay guard, and a
single authenticated operation-sequence row preserves deterministic receipt
numbering. Runtime execution still emits the same block-local receipts. LMDB
now retains each complete finalized payload, hash-bound to its block audit
record, in the same transaction as rows, authenticated state and cursors. The
benchmark JSON records both `retained_state_images` and
`retained_block_payloads`.

Three comparable runs produced 14,276.63, 11,644.47 and 11,422.89 transfers/s.
The median was **11,644.47 transfers/s**, 22.20% above the prior 9,528.65
two-phase receipt-bearing-state median. Representative median-throughput stage
totals were:

| Stage | Time |
| --- | ---: |
| Client signing and canonical envelope encoding | 49.656 ms |
| Bounded parallel batch admission | 8.195 ms |
| Deterministic proposal plus incremental root preparation | 16.550 ms |
| Cached transition/finality verification | 0.001 ms |
| Normalized rows + authenticated JMT + payload LMDB commit | 10.843 ms |
| Pool cleanup and in-memory cursor commit | 0.626 ms |

That run finalized 1,000 transfers in two blocks, retained two complete state
images and two full block payloads, and ended with a 534-byte canonical state.

A separate 10,000-transfer run sustained **14,528.65 transfers/s** over 20
blocks. Its local block latency was 34.293 ms p50, 35.131 ms p95 and 37.511 ms
p99. It retained two complete state images and all 20 full finalized payloads,
while final canonical state remained 534 bytes. This resolves the measured
receipt-driven live-state growth in the fixed-account fixture; payload archive,
authenticated-proof pruning and receipt-index retention still require capacity
and soak tests.

These results still set `network_finality:false`. They do not include validator
voting, WAN propagation, HSM/MPC signing, public APIs, receipt indexing,
merchant devices or SMS/USSD provider latency and are not production TPS or
checkout-SLO claims.
