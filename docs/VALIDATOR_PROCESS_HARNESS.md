# Local validator process harness

From the `platform` directory, run:

```text
cargo run -p validator-process-harness
```

The parent process creates ephemeral laboratory identities, starts seven child
validator processes, establishes independent mTLS connections, requests votes,
verifies peer fingerprints and assembles a real Ed25519 finality certificate.
The output contains process/respondent counts, signed/quorum weight, local vote
collection latency in microseconds and encoded proof size.

Integration tests cover five responding validators (quorum) and four responding
validators (no quorum). Temporary private material is deleted after each run.
Do not reuse the deterministic signing seeds or interpret local timing as
production finality or throughput.

Run the joined signed-payment/finality/LMDB scenario with:

```text
cargo run -p validator-process-harness -- --payment-finality
```

The default scenario contains four sequential blocks of 64 real Ed25519-signed
transfers. A bounded proposal header and hash-bound chunks are sent to five of
seven mTLS validator processes. Each validator independently executes the block
and checks its block hash and authenticated state root before a durable
reserve-before-sign precommit. The parent verifies the quorum certificate and
sends it back to every responder. Each validator verifies finality, atomically
commits its own normalized LMDB and only then acknowledges the exact checkpoint.
The parent requires all responder acknowledgements before its own atomic commit.

Validator processes are recreated between blocks and recover their checkpoint,
ledger rows and signing journal from disk. The next proposal must extend that
checkpoint and uses continuous sender nonces. At the end the parent reopens its
database and verifies every retained parent/checkpoint/payload plus the final
operation sequence. See
`ADR-0045-restart-persistent-multi-block-finality.md`.

The derived receipt index is brought through the first finalized block and then
intentionally paused while the remaining ledger blocks finalize. It is reopened
afterward and deterministically rebuilds missing receipts from the retained
payload archive. The audit verifies lookup by operation index, operation ID,
account/nonce and signed correlation data. Receipt indexing remains outside the
authoritative ledger transaction and outside core-finality timing; see
`ADR-0046-derived-receipt-index-recovery-lab.md`.

Both receipt-index passes run in separate supervised OS processes. Each worker
independently verifies the signed network configuration, opens the authoritative
ledger read path and derived index, and emits a fixed-size checksummed completion
report. The parent requires a successful exit within a bounded timeout and
rechecks all indexed lookups itself; see
`ADR-0047-supervised-receipt-worker-process.md`.

The output reports total proposal, validator-commit, coordinator-commit and
end-to-end time; p50/p95/p99/maximum block quorum and durable-finality latency;
commit acknowledgements, retained blocks and separate receipt-index work.
Timers exclude process startup, fixture/bootstrap work, receipt indexing and
child-process teardown. The scenario is a short local-host restart laboratory,
not a production TPS or latency claim; see ADR-0044 and ADR-0045.

Run the deterministic degraded-national profile with:

```text
cargo run -p validator-process-harness -- \
  --payment-finality-degraded-national
```

This uses the same real mTLS, runtime, Ed25519 finality and LMDB path while
injecting 2/50/125 ms directed one-way delays across a 3/2/2 validator layout,
one unavailable validator and one asymmetric lost return-vote path. Vote
collection advances on the first weighted quorum and cancels slower non-quorum
requests; all five selected signers must still durably commit before the
coordinator commits. It is application-layer fault injection on one host, not a
multi-host network or SLO result. See
`ADR-0048-first-quorum-network-fault-profile.md`.

Run the same degraded profile through one bounded long-lived set of validator
processes with:

```text
cargo run -p validator-process-harness -- \
  --payment-finality-persistent-degraded-national
```

The default persistent scenario runs 16 blocks of 64 signed transfers. Seven
processes start once and report zero restarts; before every later block the
coordinator verifies that all children remain alive. Each process re-reads its
authoritative finalized LMDB state for every round, and the selected five must
again verify finality and commit before acknowledgement. A bounded one-slot
pool per validator reuses healthy mTLS connections only after exact commit ack;
cancelled, timed-out or incomplete streams are discarded and lazily reconnected.
One monotonic six-second deadline covers every network phase, while concurrent
use of one validator slot fails with explicit backpressure. The report exposes
connection opens, reuse, discards, rejections and peak leases. See ADR-0049 and
ADR-0050. The pool also uses the generation-fenced health state from ADR-0051:
two transient failures put the affected laboratory peer into a deterministic
60-second backoff, while authentication or protocol failures quarantine it
until trusted configuration is rebuilt. Health-state counts are included in
the report; this remains a laboratory policy rather than a production default.

Run the authenticated state-sync recovery scenario with:

```text
cargo run -p validator-process-harness -- --state-sync
```

This starts two separate mTLS snapshot providers. The first sends one corrupted
chunk, which is rejected without advancing that chunk's progress. The second
provider supplies the canonical data; already persisted chunks are idempotent,
but before that retry the receiver destroys its in-memory session, reopens the
store and reconstructs progress from verified durable chunks. All chunks are
then re-read from storage and completion requires the exact finalized state
root. Voting and sync share the same TLS/process helpers.

The harness first signs and verifies a canonical bootstrap network
configuration. Its digest is the `ProtocolDigest` checked by peer admission and
the snapshot manifest; the same verified capability supplies the genesis
checkpoint and authenticated-root activation policy to LMDB and the runtime.

After snapshot completion, the same provider stream carries three independently
finalized tail blocks. `tail-sync-core` rejects gaps and forks, verifies each
finality proof and recomputes block/state commitments before moving from snapshot
height 100 to height 103. The synchronized snapshot is a canonical multi-asset
ledger, and every tail block contains a real canonical Ed25519-signed payment.
The runtime checks state roots, signatures, nonces and monetary invariants, then
publishes assets, balances, nonces, the operation sequence, account policies,
the canonical state image, block audit record, complete payload and finalized
cursor in one LMDB transaction. Receipts remain deterministic execution results
and can be rebuilt from the retained payload archive rather than growing live
consensus state.
Tail execution uses the incremental authenticated-state path. Immediately after
the first durable tail commit, the harness deliberately discards the uncommitted
process-local JMT update and tail cursor, closes LMDB, reopens it and reconstructs
both views from the durable checkpoint and canonical ledger snapshot. It then
continues blocks 102 and 103. This models a stop in the narrow interval after
durable publication but before process-local publication; LMDB is authoritative
and no finalized block is applied twice.
The harness closes and reopens the environment, reconstructs the normalized
ledger and requires the recovered state, authenticated root and height to match
height 103. The report exposes `tail_view_recoveries=1` for this exercised path.
