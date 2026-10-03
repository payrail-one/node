# ADR-0058: Compact consensus proposals with pre-vote data availability

## Status

Accepted and implemented for the local reference BFT path. The bounded
candidate protocol, complete-only local store, retry/fallback state machine,
consensus source adapter and live Malachite `ProposalAndParts` stream are
implemented. The local benchmark no longer preloads receiving validators.
Durable undecided-stream recovery, multi-host network shaping, operation-level
mempool reconciliation and production transport/soak evidence remain open
before testnet use.

## Context

The high-load reference block contains at most 12,288 operations and four
megabytes. Sending the same complete payload through the consensus proposal
path makes BFT serialization and dissemination compete with signature
verification and execution. It also repeats transaction data that validators
should normally have received through transaction gossip before proposal time.

Voting only for a block hash and downloading the data after finality would be
unsafe. A Byzantine proposer could finalize unavailable data, preventing honest
validators from executing, serving or recovering the chain. A compact proposal
therefore needs a separate but mandatory data-availability gate before a
validator returns `Accept`.

## Decision

Introduce `CompactLedgerBlockManifest` and extend the engine-neutral
`ConsensusPayloadSource` boundary with two responsibilities:

1. A proposer executes the complete canonical payload, then deterministically
   converts it into consensus-carried bytes. Inline payload remains the safe
   default; a gossip-backed source may emit a compact manifest.
2. A receiving validator resolves the consensus bytes to the complete payload.
   Missing data produces `Reject` before any vote. Resolver errors fail closed.
3. The application treats resolved bytes as untrusted. It re-executes all
   signed operations and must reproduce the proposed block hash and state root
   before returning `Accept`.
4. A bounded side-effect-free execution cache binds the exact consensus value,
   parent and proposed checkpoint to the resolved complete payload. It may be
   reused only after independent finality verification.
5. Finalization passes the complete payload through `TailSyncSession`; LMDB
   atomically stores normalized mutations, authenticated state, finalized block
   record, complete payload and cursor. A manifest is never stored in place of
   the finalized block.

Live dissemination uses Malachite's engine-native
[`ProposalAndParts`](https://github.com/circlefin/malachite/blob/main/docs/architecture/adr-003-values-propagation.md)
path, following its value-propagation boundary rather than introducing a
second consensus-adjacent transport. One bounded stream contains:

1. `Prelude`: exact height, round, expected proposer, protocol, parent,
   manifest and chunk layout;
2. `ManifestSeal`: the proposer's Ed25519 signature over that exact prelude;
3. ordered `Data` chunks, each bounded to at most 256 KiB;
4. `AvailabilitySeal`: the proposer's Ed25519 signature over the complete
   prelude-and-data transcript;
5. `Value`: the compact consensus value produced after proposer execution;
6. `Seal`: the proposer's signature over the complete transcript.

Adding `ManifestSeal` is wire-incompatible. The proposal-part codec,
transcript-signing domain and network protocol namespace are therefore bumped
to v2; mixed v1/v2 validators cannot silently share a consensus channel.

The payload source publishes `Prelude` and `ManifestSeal` immediately after
selecting the candidate, before constructing the full fallback transcript or
executing the proposal. A receiver may use this authenticated descriptor to
reconstruct the exact payload from a bounded local pool. A miss waits for the
ordered `Data` chunks and `AvailabilitySeal`. Either route publishes only an
exact verified payload to the bounded store and may execute it speculatively
against the exact parent. It does not emit `ProposedValue` and cannot prevote
yet. Only the later `Value` plus final `Seal`, exact manifest/value binding and
exact reproduced block hash and state root allow the normal application
validation result to reach consensus. Thus early execution overlaps
distribution/proposer execution without converting availability into authority.

There are at most 128 active inbound streams, 128 pending/cached local streams
and the existing candidate-store byte/count limits. Sequence numbers, stream
layout, scope, proposer selection, both transcript signatures and all compact
commitments fail closed. An HSM/remote-signer interface owns proposal-part
signing; the benchmark's software key is only one implementation.

The pre-vote retrieval foundation is a separate `block-availability-core`
domain boundary:

- `CandidateDescriptor` binds network, membership epoch, protocol, exact parent
  checkpoint, complete manifest, payload hash/length and canonical chunk layout
  into one domain-separated candidate identifier;
- protocol frames are preflight-bounded before allocation. The current defaults
  permit a four-megabyte payload as at most sixteen 256 KiB chunks, 32 peers,
  four provider attempts, eight aggregate concurrent requests, 128 retained
  complete candidates and 512 MiB of retained complete bytes;
- the event-driven fetch session performs no transport I/O itself. A network
  driver obtains an exact request, applies its deadline, then returns a decoded
  response together with the authenticated `Admission` of the serving peer;
- a response is accepted only from a current validator in the exact network,
  epoch and protocol. Request ID, parent, candidate, payload hash, total length,
  chunk layout and position must all match;
- timeout discards the current provider's partial candidate and permits a
  bounded fallback. A protocol violation or corrupt complete reconstruction
  quarantines only that provider through `PeerManager`; an honest fallback can
  still complete;
- `CandidateAvailabilityStore` contains complete verified payloads only.
  Partial chunks remain session-local and disappear on restart. The synchronous
  consensus resolver only reads this store and never waits for network I/O;
- a proposer publishes its exact complete candidate before emitting the compact
  manifest. A validator without the candidate still returns `Reject`.

The canonical manifest contains:

- a domain-separated format identifier;
- the exact complete-payload length;
- a domain-separated SHA-256 commitment to every payload byte, including all
  authorizations;
- the exact ordered list of 32-byte operation identifiers.

At the current maximum operation count the manifest is 393,280 bytes, compared
with 4,005,908 bytes for the observed maximum simple-transfer payload. This is
about a tenfold reduction before outer consensus framing. Short transaction IDs
are deferred: collision negotiation and adversarial ambiguity handling must be
specified before they can safely replace full operation identifiers.

## Security properties

- A manifest is an availability descriptor, not proof of valid execution.
- Validators never vote from the manifest alone. They must possess the full
  payload and execute it successfully against the exact finalized parent.
- The operation IDs bind ordered monetary operations, while the payload hash
  additionally binds signatures and envelope framing.
- The runtime block hash independently binds the parent and exact payload; the
  state root binds deterministic execution output.
- Engine finality evidence remains bound to the compact consensus value. The
  finalized storage record remains bound to the reconstructed full block.
- A non-validator follower may download a finalized block later, but a voting
  validator may not defer availability until after its vote.

Validity proofs or data-availability sampling could eventually permit different
node roles, but neither replaces complete data availability for validators in
this phase.

## Verification and current limitation

Runtime tests prove canonical manifest round-trip and rejection of payload or
operation-list tampering. Consensus-application tests use distinct proposer and
validator LMDB stores: a validator without the payload rejects before voting;
one with the exact payload re-executes, accepts, finalizes and retains the full
payload rather than the manifest. A dedicated speculative-execution regression
test proves that a matching final value reuses the verified transition, while a
value with a forged state root is rejected without a second signature check or
storage mutation. A proposer-authenticated candidate containing an invalid
payment remains a nonfatal speculative miss and is rejected by normal value
validation; it cannot terminate the receiving host actor.
A real three-validator Linux/QUIC test gives every node a different valid local
candidate for the same height. Whichever authenticated proposer wins, the two
other validators reconstruct that exact foreign payload, reproduce its state
transition and all three reopened LMDB stores contain byte-identical finalized
payloads.

Focused availability tests cover exact reconstruction, complete-only
publication, authenticated serving, corrupt-first-provider fallback, timeout
exhaustion, wrong network/epoch/parent/candidate/hash/length/layout/index,
oversized frames, duplicate/reordered chunks, non-validator authorization,
retention limits and restart with partial data. The implementation currently
retrieves one candidate sequentially from one provider at a time; bounded
parallel chunk scheduling and operation-level reconciliation for partially
overlapping mempools remain follow-up work rather than hidden unbounded paths.

The three-validator Linux lab now starts every receiver with an empty candidate
store and transmits the full 4,005,908-byte payload through the live signed
proposal-part stream. The compact 393,280-byte manifest is still the consensus
value. The proposer starts dissemination before its own execution, and
receivers speculatively execute only after validating `AvailabilitySeal`.
Benchmark JSON labels this mode
`live_malachite_proposal_parts_speculative` and emits both byte sizes.

The historical preloaded 10-core median was 51,248.43 finalized transfers/s;
it remains useful execution/consensus evidence but is not live distribution.
The constrained live rerun uses only 6 shared Linux/arm64 VM vCPUs while
unrelated containers remain active. Three instrumented release runs of 36,864
transfers with 12 signature workers per validator produced 43,859.88 /
41,704.75 / 42,880.38 finalized transfers/s: median 42,880.38 with 293.06 ms
median run-level p95
durable finality. This closes the preloading gap, not the 65,000 gate. It
exposes the next bottleneck: three validators independently perform strict
Ed25519 verification and execution while contending for six CPUs. Production
work still needs durable undecided-part/value persistence, adversarial
live-stream fault tests, partially overlapping mempool reconciliation and
repeated separate-host runs with network shaping.

The same VM executes the canonical runtime at a 202,149.06 transfers/s median
for one validator. At the 65k target, one maximum block has a 189.05 ms budget;
three independent measured validations consume about 182.36 ms before charging
distribution, BFT messages or durable commits. This makes additional exclusive
cores or separate hosts a prerequisite for the next honest capacity attempt on
this workload.

The follow-up steady-state profile moves that already-required Ed25519 work to
independent pre-consensus admission capabilities, uses no payload preload, and
selects one-hop broadcast for the fully connected permissioned committee.
Three valid live-payload runs measured a 96,832.62 finalized transfers/s median
and 136.71 ms median run-level p95 durable finality. This crosses the 65,000
consensus-capacity gate while retaining exact pre-vote payload availability,
execution and LMDB commit. It does not include client ingress, gossip or the
earlier signature-verification CPU in the timed interval; that sustained
client-to-finality gate remains open under ADR-0059.

The instrumented run separates overlapping phases. Median run-level p50/p95
was 136.94/162.50 ms from payload selection until authenticated receiver
execution start, 95.93/99.88 ms for proposer build, 116.53/122.25 ms for
receiver speculative execution, 0.89/1.18 ms for final cached validation and
12.49/17.35 ms for LMDB commit. The measured critical path is therefore live
delivery followed by independent receiver execution. ADR-0059 implements the
bounded process-local preverified-authorization foundation and specifies the
next exact-envelope reconciliation increment. Live transport integration and a
sustained non-preloaded benchmark remain open; monetary and parent-dependent
rules continue to execute in deterministic order.
