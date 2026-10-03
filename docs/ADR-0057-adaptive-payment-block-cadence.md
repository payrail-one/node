# ADR-0057: Adaptive payment-block cadence with a 12-second idle heartbeat

## Status

Accepted for the reference implementation. The numeric production parameters
remain provisional until separate-host and multi-region fault benchmarks pass.

## Historical evidence

The legacy implementation reads `MINE_INTERVAL_SECONDS` and starts a JavaScript
interval timer at that fixed period. Its checked-in example configuration sets
the value to 12 seconds. The original Temtum design was therefore implemented
as one selected leader producing five blocks during a 60-second leadership
window.

This matches the official historical description: the Temtum site calls 12
seconds the researched block-generation interval, while the whitepaper calls it
the maximum time for a transaction to be included and immediately confirmed.
The same material removes the block-size limit and attributes the remaining
capacity limits to hardware and bandwidth. Sources: [Temtum platform
description](https://temtum.com/platform/) and [Temtum
whitepaper](https://temtum.com/downloads/temtum-whitepaper.pdf).

That design is valuable migration evidence, but it is not a safe production
limit for the new BFT payment runtime. An unbounded block permits memory,
validation-time and network-amplification denial of service. A fixed 12-second
payment slot also gives uniformly arriving transactions an average inclusion
wait of about six seconds and a p95 wait of about 11.4 seconds before consensus
and storage time are added.

Ethereum also uses 12-second slots, but for a very different globally open
validator design with committee attestations and a fork-choice/finality model.
It does not establish 12 seconds as an appropriate merchant-payment latency for
a permissioned deterministic-finality network. Source: [Ethereum proof of
stake](https://ethereum.org/developers/docs/consensus-mechanisms/pos/).

Malachite exposes proposal, prevote and precommit timeouts. Its application is
expected to supply a value within the proposal timeout; it does not require a
12-second application block timer. Modern responsive BFT research likewise
distinguishes safety timeouts from steady-state progress at actual network
speed. Sources: [Malachite consensus API](https://docs.rs/crate/informalsystems-malachitebft-core-consensus/latest)
and the [HotStuff paper](https://arxiv.org/abs/1803.05069).

## Capacity consequence

The reference format now admits at most 12,288 operations and four megabytes
per block. A max-size canonical simple-transfer block observed in the local
benchmark is 4,005,908 bytes.

- one such block every 12 seconds is only 1,024 transfers/s;
- 65,000 transfers/s at a 12-second cadence requires 780,000 transfers in one
  block;
- at the current roughly 326-byte envelope size that would be about 254 MB
  before consensus framing, far outside the bounded protocol.

The old 12-second payment cadence and the new 65,000 finalized-transfer target
therefore cannot both be protocol invariants.

## Decision

Use an adaptive, bounded proposal policy:

1. Propose immediately when the candidate block reaches its operation or byte
   capacity.
2. Once the first eligible transaction is waiting, allow at most 250 ms for
   batching, then propose the partial block.
3. Start the next height as soon as the preceding height is durably finalized;
   consensus timeouts protect liveness and do not impose an artificial
   successful-round delay.
4. When there are no transactions, emit at most one empty maintenance heartbeat
   every 12 seconds. The heartbeat is for liveness, height-based maintenance and
   observability; it is not the customer payment interval.
5. A merchant receives `final` only after the BFT decision and authoritative
   durable commit. A faster API acknowledgement must remain explicitly
   `accepted`, never a disguised final confirmation.

At 65,000 transfers/s a 12,288-operation block fills in about 189 ms, so the
capacity trigger fires before the 250 ms low-load batching limit. Both limits
remain bounded by the four-megabyte payload guard.

The policy is implemented in `block-production-core` as
`BlockCadencePolicy`. Proposal scheduling is a local performance choice and
does not change block validity. Before a public testnet, its network defaults
must be carried by signed configuration so operators cannot accidentally deploy
incompatible latency or resource profiles.

## Security and time semantics

- Consensus propose/prevote/precommit timeouts remain independent and increase
  on failed rounds; they must be derived from measured regional latency.
- A proposer clock decides only when to submit an otherwise valid candidate.
  It cannot alter ordering or monetary execution rules.
- Smart-contract time must later use a separately specified bounded consensus
  timestamp. Neither a local wall clock nor the 12-second heartbeat is an
  authoritative contract clock.
- The operation-count and payload-byte bounds remain mandatory even if larger
  blocks improve a local throughput benchmark.
- No speculative receipt is described as final to hide a slow or failed BFT
  round.

## Verification gates

Before production parameters are frozen, run the same signed-payment workload
with maximum batching delays of 50, 100, 200, 250, 500 and 1,000 ms under:

- low, normal, burst and sustained saturation loads;
- three, seven and larger validator committees;
- separate hosts in one region and three geographic regions;
- slow/offline proposers, loss, partition, HSM delay and LMDB pressure;
- compact and full proposal dissemination.

Select the largest batching window that still holds p95 irreversible finality
below one second and p95 merchant acknowledgement below 500 ms while meeting
the sustained throughput target. The accepted 250 ms value is the current
reference starting point, not a marketing guarantee.
