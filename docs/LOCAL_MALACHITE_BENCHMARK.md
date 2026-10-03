# Local Malachite payment benchmark

## Status

This is an engineering diagnostic from an uncommitted development snapshot. It
is not a production, public-testnet or Visa-equivalent capacity claim. The
result becomes publishable only after the source revision, container image and
complete run artifacts are pinned and an independent party reproduces it.

## What is measured

The test runs three validator actors with the pinned Malachite BFT engine. Each
validator has its own consensus and transport identity, signer journal, WAL and
LMDB ledger. Consensus messages travel through the real permissioned
QUIC/libp2p transport over loopback.

The workload contains canonical Ed25519-signed payment operations. The measured
interval begins when the first measured proposal is built and ends only after
the last validator has durably committed the final measured block to LMDB.
Per-block finality latency begins at proposal build and ends at each validator's
successful atomic ledger commit. Confirmation skew is the difference between
the first and last validator commit for the same height.

Three finalized empty blocks warm the already authenticated network before the
measurement. This separates steady-state payment latency from cold consensus
startup. Cold startup remains a separate recovery SLO and is not discarded:
before the warm-up was added, the first height consistently took about 3.5
seconds while subsequent one-payment blocks completed in milliseconds.

The benchmark excludes client/API latency, operation signing, admission and
mempool time, WAN delay, HSM latency, indexers, fraud checks and notifications.
All operations are prepared before timing. After every run, all three nodes are
stopped, each LMDB is reopened, and its final checkpoint, balances, nonce and
retained block count are checked.

## Environment

- Apple M1 Max, 10 CPU cores, 64 GB RAM;
- Docker Server 29.2.1, Linux arm64;
- Rust 1.88 release build;
- three validators in one OS process with independent durable stores;
- local loopback QUIC, with no injected delay, loss or bandwidth limit.

## Results

### Current live compact high-load profile (22 September 2026)

Receiving validators now begin without the proposed payload. Malachite's native
`ProposalAndParts` stream carries an authenticated `Prelude`, all bounded data
chunks and an `AvailabilitySeal` before the proposer finishes execution. A
receiver verifies the expected proposer and complete transcript, reconstructs
the exact payload, publishes it to a complete-only bounded store and executes
it speculatively. It still withholds `ProposedValue` and therefore cannot vote
until the later compact value and final transcript seal arrive and both block
hash and state root exactly match.

The JSON output labels this path
`live_malachite_proposal_parts_speculative`. Every 12,288-operation block moves
the 4,005,908-byte full payload live, while the consensus value remains the
393,280-byte manifest. This removes benchmark preloading without weakening the
pre-vote availability or execution gate.

The same Linux path has a focused distinct-source test: all three nodes start
with different valid local candidates for one height, yet the two receivers
obtain the winning proposer's exact bytes through the authenticated stream and
all reopened LMDB stores retain the same finalized full payload.

The constrained Linux/arm64 Colima VM exposed only 6 vCPUs and 12.5 GB RAM;
three validators shared those CPUs with unrelated active development
containers. An initial six-worker short series produced 37,940.94 / 37,826.81 /
33,162.26 finalized transfers/s. Extending the one-block sweep gave 42,259.88
at 10 workers, 42,919.74 at 12 and 39,985.86 at 16. The comparable three-block
series therefore uses 12 signature workers per validator.

Each release run below finalizes 36,864 transfers in three measured maximum
blocks after the same three warm-up blocks used by the historical profiles.
The interval still ends only after the slowest validator's durable LMDB commit.

| Run | Finalized TPS | Finalized tx/min | Finality p50 | Finality p95 | LMDB p50/p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 43,859.88 | 2,631,592.63 | 283.15 ms | 287.09 ms | 12.49 / 17.35 ms |
| 2 | 41,704.75 | 2,502,285.21 | 291.45 ms | 299.97 ms | 12.58 / 21.07 ms |
| 3 | 42,880.38 | 2,572,822.93 | 285.49 ms | 293.06 ms | 12.13 / 15.70 ms |
| Median | **42,880.38** | **2,572,822.93** | **285.49 ms** | **293.06 ms** | **12.49 / 17.35 ms** |

Median run-level p95 validator confirmation skew was 0.49 ms. The live median
is 83.7% of the historical preloaded compact median and 66.0% of the 65,000
engineering gate. Selecting the measured worker optimum and avoiding redundant
final reconstruction improved the first six-worker live median. The remaining
delta is now measured distribution/scheduling/CPU contention rather than hidden
benchmark setup. The later instrumented series is 6.4% above the preceding
uninstrumented three-run median; this is normal shared-host variance and is not
attributed to instrumentation.

The instrumented JSON now separates overlapping phases. Values below are the
medians of each run-level percentile; they must not be summed because proposer
build, distribution and receiver work overlap.

| Phase | Median p50 | Median p95 |
| --- | ---: | ---: |
| Payload selection → authenticated receiver execution start | 136.94 ms | 162.50 ms |
| Proposer build and deterministic execution | 95.93 ms | 99.88 ms |
| Receiver speculative deterministic execution | 116.53 ms | 122.25 ms |
| Final compact-value validation from exact cached transition | 0.89 ms | 1.18 ms |
| Proposal → durable finality | 285.49 ms | 293.06 ms |
| Atomic LMDB commit | 12.49 ms | 17.35 ms |

The critical path is therefore authenticated payload delivery followed by
receiver execution, not final compact-value validation or LMDB. The next
performance experiment should pre-gossip and independently preverify exact
operation envelopes, then overlap missing-envelope reconciliation with the
preceding height. Any process-local authorization cache must bind the complete
signed envelope and network, remain bounded, and still reapply all monetary and
parent-dependent rules in deterministic block order.

### Native authorization-cache diagnostic (22 September 2026)

ADR-0059 adds that exact process-local capability cache. A release diagnostic
on the 10-core M1 Max ran three 12,288-operation blocks through the canonical
single-validator runtime. It is not network TPS: transaction gossip, Malachite,
QUIC and LMDB are excluded, and preverification time is reported separately.

Strict in-block verification produced 247,554.36 / 240,688.49 / 240,420.79
executed transfers/s, with a 240,688.49 median and 51.22 ms median run-level
block p50. Preverifying every exact envelope immediately before its block and
then executing from the bounded cache produced 592,700.17 / 606,518.22 /
591,181.75 state-transition transfers/s, with a 592,700.17 median and 20.58 ms
median run-level block p50. Median preverification time was 190.99 ms for
36,864 signatures and remains real validator CPU work; it is moved out of the
block critical path, not deleted.

The diagnostic therefore shows approximately 2.46x critical-path execution
headroom when gossip has already completed local authorization. It does not by
itself raise total same-host CPU capacity or close the 65k network gate. The
next proof must overlap that work with transaction arrival and reconstruct the
proposal from the same live-gossiped envelopes.

A companion single-validator runtime check in the same 6-vCPU VM produced
212,876.91 / 202,149.06 / 201,591.63 executed transfers/s, median 202,149.06.
Its median three-block elapsed time was 182.36 ms, or about 60.79 ms per maximum
block. A 12,288-operation block has only a 189.05 ms total budget at 65,000
finalized transfers/s. Three independent validations at the measured median
would consume about 182.36 ms even if scheduled perfectly, leaving roughly 6.69
ms for 4 MiB dissemination, proposal/votes and three durable commits. This is
not a capacity estimate, but it explains why 65k is effectively at the physical
ceiling of this six-core shared-host topology. The next honest proof needs more
exclusive cores or separate validator hosts, not a benchmark bypass.

This is honest live-distribution evidence, but it does not reach the 65,000
gate and is not a Visa-equivalent claim. In this shared-CPU topology, the next
bottleneck is the three validators independently performing strict Ed25519
verification and deterministic execution on six total vCPUs. The next capacity
proof needs exclusive higher-core Linux hardware or three separate hosts;
changing the security checks or preloading data is not an acceptable shortcut.

### Preverified steady-state live profile (22 September 2026)

The process-local authorization capability from ADR-0059 is now connected to
each validator executor. For this profile every validator independently verifies
the exact signed envelopes before the measured first proposal. Cache hits skip
only repeated Ed25519 work; ordered nonce, replay, balance, fee, expiry,
deterministic state transition, commitments, BFT finality and LMDB commit still
run normally. Receiving availability stores are empty: the full 4,005,908-byte
payload per maximum block is transferred live. The output therefore reports
`preloaded_reconstruction_diagnostic:false` and
`preverified_authorizations_diagnostic:true`.

The fixed three-node, fully connected permissioned topology selects Malachite's
one-hop broadcast implementation over QUIC rather than GossipSub forwarding.
This removes mesh overhead but does not change scoped signatures, authenticated
peer admission, proposal validation, quorum thresholds or the pre-vote complete
payload/execution gate. It is safe only while every validator maintains a direct
connection to every other committee member.

Three valid release runs on the same constrained 6-vCPU Linux/arm64 VM finalized
36,864 transfers in three measured 12,288-operation blocks:

| Run | Finalized TPS | Finality p50 | Finality p95 | LMDB p50/p95 |
| --- | ---: | ---: | ---: | ---: |
| 1 | 96,832.62 | 120.90 ms | 136.71 ms | 9.15 / 10.08 ms |
| 2 | 93,047.54 | 121.39 ms | 156.91 ms | 9.54 / 10.85 ms |
| 3 | 98,059.14 | 124.82 ms | 128.42 ms | 9.90 / 11.51 ms |
| Median | **96,832.62** | **121.39 ms** | **136.71 ms** | **9.54 / 10.85 ms** |

This is 49.0% above the 65,000 steady-state finalized-throughput target. It is a
real live-network, three-validator, durable-finality result and does not preload
the proposed payload. It is not yet an end-to-end Visa-equivalent claim: the
timed interval begins at proposal construction, after transaction generation,
admission and independent signature preverification. A sustained pipeline must
next include client submission, transaction gossip and that preverification in
the measured interval and demonstrate that their queues remain bounded.

### Historical preloaded compact high-load profile

This historical profile modeled completed transaction gossip by preloading the
exact complete block payload on every validator before consensus. Malachite
carries a canonical manifest containing the exact payload length, payload hash
and 12,288 ordered operation IDs. A validator resolves the preloaded payload,
checks it against the manifest and executes it before accepting the proposal.
LMDB still retains the complete block after finality.

This reduces the maximum consensus-carried payload from 4,005,908 to 393,280
bytes. It does not measure transaction gossip, missing-envelope retrieval or
partially overlapping mempools; the JSON output labels the distribution mode
as `preloaded_before_consensus`.

| Run | Finalized TPS | Finalized tx/min | Finality p50 | Finality p95 | LMDB p50/p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 53,350.73 | 3,201,043.83 | 230.43 ms | 238.81 ms | 10.94 / 13.94 ms |
| 2 | 51,248.43 | 3,074,905.66 | 236.15 ms | 243.80 ms | 12.58 / 13.80 ms |
| 3 | 50,647.51 | 3,038,850.36 | 245.14 ms | 246.06 ms | 12.47 / 13.62 ms |
| Median | **51,248.43** | **3,074,905.66** | **236.15 ms** | **243.80 ms** | **12.47 / 13.80 ms** |

Median run-level p95 validator confirmation skew was 1.16 ms. The compact
profile is 39.3% faster than the preceding full-payload median and reaches 78.8%
of the 65,000 finalized-transfer/s engineering gate. The network gate remains
open.

### Constrained Linux worker-count diagnostic (22 September 2026)

A single-sample worker-count sweep was run inside the local Linux/arm64 Colima
VM after making the benchmark worker count explicit. The VM had only 6 vCPUs
and 12.5 GB RAM while unrelated development containers remained active, so
these samples are neither a replacement for the 10-core three-run baseline nor
a new capacity claim. Payload distribution was still
`preloaded_before_consensus`.

| Signature workers / validator | Finalized transfers/s | Run p95 finality |
| ---: | ---: | ---: |
| 1 | 21,201.36 | 584.70 ms |
| 2 | 36,074.45 | 341.05 ms |
| 3 | 44,488.04 | 304.65 ms |
| 4 | 43,602.07 | 330.82 ms |
| 5 | **53,741.97** | 240.18 ms |
| 6 | 53,409.50 | **231.58 ms** |
| 8 | 51,886.68 | 251.33 ms |
| 10 | 53,684.97 | 238.07 ms |
| 12 | 50,545.50 | 248.19 ms |
| 16 | 52,692.30 | 238.42 ms |

The sweep shows that one or two workers underutilize signature verification and
that the previous fixed value of eight is not universally optimal. Five to ten
workers were effectively tied within single-run noise. Two immediate repeats at
five workers produced 46,240.82 and 42,803.05 transfers/s, making the three-run
median **46,240.82 transfers/s** (range 42,803.05–53,741.97). This variance is
consistent with a CPU-constrained VM shared with unrelated active containers.
Even the best sample was below 65,000. It predates the live-distribution profile
above and must not be mixed with it.

### Preceding full-payload high-load profile

The preceding optimized profile used strict Ed25519 batch verification, the
library's fixed-base acceleration tables, and a bounded side-effect-free cache
of the exact transition already produced while building or validating the
proposal. Finality evidence is still verified before the cached transition can
reach `TailSyncSession`; the exact parent, value identity, payload, block hash
and state root must match, and LMDB is still the publication boundary.

The bounded block limit is now 12,288 operations and four megabytes. Each run
below finalizes 36,864 transfers in three measured blocks after three warm-up
blocks. All three validators share one 10-core host and each has a configured
maximum of eight signature workers, so this is intentionally a contention-heavy
local test rather than a separate-host capacity result.

| Run | Finalized TPS | Finalized tx/min | Finality p50 | Finality p95 | LMDB p50/p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 36,787.04 | 2,207,222.23 | 323.79 ms | 352.27 ms | 10.46 / 12.90 ms |
| 2 | 36,507.41 | 2,190,444.47 | 331.37 ms | 344.88 ms | 11.82 / 12.40 ms |
| 3 | 37,367.29 | 2,242,037.36 | 329.58 ms | 335.81 ms | 12.29 / 14.68 ms |
| Median | **36,787.04** | **2,207,222.23** | **329.58 ms** | **344.88 ms** | **11.82 / 12.90 ms** |

Every maximum payload was 4,005,908 bytes. Median run-level p95 validator
confirmation skew was 0.49 ms. The result was 56.6% of the 65,000 finalized
transfer/s engineering gate.

### Single-validator canonical runtime gate

The companion block-runtime profile isolates the deterministic work that every
validator must perform: bounded block decode, strict/batched Ed25519 checks,
ordered monetary execution, and block/authenticated-state commitments. Signed
input is prepared before timing, and final balances and nonce are checked after
all 16 blocks. It excludes consensus, transport and LMDB.

| Run | Executed TPS | Block execution p50 | Block execution p95 |
| --- | ---: | ---: | ---: |
| 1 | 263,210.24 | 46.11 ms | 55.19 ms |
| 2 | 263,223.74 | 46.58 ms | 49.24 ms |
| 3 | 265,539.03 | 46.39 ms | 52.21 ms |
| Median | **263,223.74** | **46.39 ms** | **52.21 ms** |

The local runtime gate is therefore more than four times the 65,000 target.
The remaining gap is in consensus scheduling and three validators competing
for one host. Live bounded dissemination has now replaced preloading in the
current profile above; the next capacity proof must use exclusive or separate
machines. The single-validator number must not be presented as network TPS.

### Batched capacity profile

Each run finalizes 10,000 transfers in 20 measured blocks of 500 operations.
The current profiled series contains five runs.

| Run | Finalized TPS | Finalized tx/min | Finality p50 | Finality p95 | Validator skew p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 8,709.96 | 522,597.36 | 53.00 ms | 67.83 ms | 4.49 ms |
| 2 | 8,620.62 | 517,237.19 | 55.63 ms | 67.55 ms | 7.61 ms |
| 3 | 8,968.52 | 538,111.31 | 54.20 ms | 62.67 ms | 6.51 ms |
| 4 | 9,133.41 | 548,004.50 | 53.21 ms | 58.83 ms | 0.74 ms |
| 5 | 9,242.77 | 554,566.10 | 52.68 ms | 55.75 ms | 3.61 ms |
| Median | **8,968.52** | **538,111.31** | **53.21 ms** | **62.67 ms** | **4.49 ms** |

The median is a median of the three run-level measurements. It is a useful
local capacity baseline, not a sustained multi-host result.

The earlier baseline executed an ordered block twice in succession during
`verify_and_commit`: once for an explicit commitment comparison and again in
`TailSyncSession`. The protected tail transition already performs the complete
signed runtime execution and compares both commitments before allowing storage.
Removing only that redundant first execution raised the immediate like-for-like
three-run median from 6,553.43 to 7,798.94 transfers/s (about 19%) and reduced
median run-level p95 finality from 93.38 to 83.33 ms. The later five-run profiled
series above is faster, but local-machine variance means the additional change
must not be attributed to that code edit. A counting-verifier regression test
proves that bad finality performs no runtime verification and a valid ordered
value performs exactly one verification in the commit path.

### Durable storage profile

The same five runs measure only the synchronous `commit_verified_ledger` call
that atomically publishes normalized rows, authenticated state, full block
payload and finalized cursor.

| Run | LMDB commit p50 | LMDB commit p95 | LMDB commit p99/max |
| --- | ---: | ---: | ---: |
| 1 | 1.80 ms | 3.99 ms | 4.19 ms |
| 2 | 1.83 ms | 3.85 ms | 10.03 ms |
| 3 | 1.67 ms | 2.86 ms | 4.25 ms |
| 4 | 1.81 ms | 2.40 ms | 2.88 ms |
| 5 | 1.75 ms | 2.95 ms | 3.25 ms |
| Median | **1.80 ms** | **2.95 ms** | **4.19 ms** |

LMDB commit is therefore not the dominant cost in this local profile. Runtime
execution/signature verification and consensus dissemination must be measured
and optimized before changing the authoritative storage design.

### One payment per block profile

Each run finalizes 100 transfers in 100 measured blocks. Throughput in this
profile measures sequential block cadence, not network capacity.

| Run | Blocks/s | Finality p50 | Finality p95 | Finality p99 | Validator skew p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 93.55 | 9.74 ms | 10.86 ms | 11.52 ms | 0.36 ms |
| 2 | 92.21 | 9.70 ms | 11.83 ms | 14.15 ms | 0.44 ms |
| 3 | 84.66 | 10.17 ms | 13.59 ms | 19.07 ms | 0.84 ms |
| Median | **92.21** | **9.74 ms** | **11.83 ms** | **14.15 ms** | **0.44 ms** |

The largest observed steady-state block latency was 51.05 ms; run-level p99
remained at or below 19.07 ms. This does not predict latency between
geographically separated validators.

## Comparison boundary

Visa says its network can process up to 83,000 transaction messages per second.
Those are card-network messages, not directly equivalent to irreversible
on-chain transfers. Visa also reports 257.5 billion transactions processed on
its networks in fiscal 2025, an annual average of roughly 8,200 per second.
Neither figure can be compared directly with this local finalized-transfer
test. Sources: [Visa infrastructure overview](https://corporate.visa.com/en/sites/visa-perspectives/security-trust/inside-visa-global-commerce-engine.html)
and [Visa 2025 financial highlights](https://annualreport.visa.com/financials/default.aspx).

The engineering scale gates are therefore explicit:

1. sustain at least 10,000 finalized simple transfers/s on separate hosts with
   realistic regional network conditions;
2. reach p95 merchant acknowledgement below 500 ms and p95 finality below one
   second without hiding delayed or failed finality;
3. demonstrate at least 65,000 finalized simple transfers/s in the defined
   batched profile and at least 100,000 payment messages/s burst capacity;
4. preserve correctness and bounded latency during validator loss, partitions,
   restart, catch-up, HSM delay and storage pressure;
5. run a 30-day soak and have the methodology independently reproduced.

## Next engineering work

- profile execution, signature verification, consensus serialization and LMDB
  commit cost separately;
- persist undecided proposal parts/values and add adversarial live-stream fault
  and restart tests;
- connect the implemented local manifest-order reconstruction to bounded
  authenticated missing-envelope requests for partially overlapping mempools,
  then stop sending already-known envelopes in the common proposal path;
- add parallel verification and deterministic parallel execution for
  non-conflicting accounts;
- pipeline proposal construction, block dissemination and durable commit;
- run the same workload with seven independent validator processes;
- repeat it across three network-delay regions with loss and bandwidth limits;
- add API, mempool, HSM, fraud and explorer stages to the end-to-end profile;
- pin the source revision and produce machine-readable run artifacts.

## Reproduction

The ignored Linux integration benchmark is
`malachite-consensus-adapter::local_three_validator_finalized_throughput`.
Configure it with:

```text
MALACHITE_BENCHMARK_OPERATIONS=10000
MALACHITE_BENCHMARK_BLOCK_OPERATIONS=500
MALACHITE_BENCHMARK_SIGNATURE_WORKERS=12
```

For the preverified steady-state profile above use 36,864 operations, 12,288
operations per block, eight signature workers, and:

```text
MALACHITE_BENCHMARK_DIRECT_BROADCAST=1
MALACHITE_BENCHMARK_PREVERIFIED_AUTHORIZATIONS=1
```

Keep `MALACHITE_BENCHMARK_PRELOADED_RECONSTRUCTION` unset. Setting it changes
the run into a payload-preload diagnostic and invalidates the live-distribution
claim.

Use `100` and `1` respectively for the one-payment-per-block profile. Run the
test with `--release --ignored --nocapture`; its output is one JSON object per
run and is intended to be captured verbatim by the future benchmark runner.

The non-network authorization diagnostic can be reproduced separately with:

```text
cargo run --release -p payment-benchmark -- --block-runtime \
  --operations 36864 --block-operations 12288 \
  --preverified-authorizations
```
