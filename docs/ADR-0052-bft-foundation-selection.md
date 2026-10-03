# ADR-0052: BFT foundation selection

## Status

Accepted for an integration spike. Malachite is the first candidate, not yet the
production selection. Production remains gated by licensing, independent
security review and a common multi-host bake-off.

## Context

The platform needs deterministic BFT finality for a sovereign network with a
permissioned or consortium validator set. The payment runtime, monetary rules,
LMDB state and public transaction format must remain independent of the
consensus implementation.

Implementing a new consensus protocol would make the project responsible for
proving safety, liveness, recovery and validator-set transitions from scratch.
Forking a complete L1 monorepo would instead couple the payment model to its VM,
storage, governance and licensing.

The current laboratory quorum protocol remains useful for executable
invariants, proof verification and fault injection, but is not a production
consensus implementation.

## Decision

1. Introduce a framework-neutral `ConsensusEngine` integration boundary around
   proposal construction, deterministic verification, ordered finalization,
   committee transitions, recovery and evidence reporting.
2. Use pinned upstream
   [Circle Malachite](https://github.com/circlefin/malachite) as the first
   integration candidate. It is an Apache-2.0 Rust Tendermint BFT library that
   accepts an application state machine.
3. Do not reimplement or copy Malachite to create a nominally proprietary
   consensus. Our proprietary value remains in the payment runtime, ledger,
   policies, APIs and operations.
4. Maintain an internal source mirror and exact commit/SBOM. A private fork is
   allowed only for minimal reviewed fixes that are not yet available upstream;
   each divergence requires an owner, regression test and upstreaming decision.
5. Implement equivalent bake-off adapters for Commonware Simplex, Polkadot SDK
   and CometBFT before the production decision.
6. Keep finality proof verification independent from the live engine so state
   sync, light clients and auditors can verify historical finality evidence.
7. Start the spike from the immutable `v0.8.0` commit. Its declared Rust 1.88
   minimum requires an explicit workspace MSRV/toolchain update before the
   dependency enters production code.

## Implemented slice

- `consensus-engine-core` owns canonical platform values, stable value IDs,
  bounded authenticated messages, bounded untrusted finality evidence and the
  engine/application traits.
- `malachite-consensus-adapter` pins commit
  `72143f6c99a98452b587e1c392bdb80944eb2232` and implements Malachite's upstream
  `Context` and `Value` traits using only canonical platform data.
- The adapter converts the canonical platform committee into a structurally
  non-empty weighted Malachite validator set and checks the exact network,
  membership epoch and validator-set hash before engine startup.
- Proposal and vote preimages have distinct domains and bind the network,
  membership epoch, validator-set hash, height, round, value and signer. The
  Ed25519 verifier is constructed with the expected local scope and treats
  foreign scopes, malformed encodings and keys as invalid peer input.
- The first protocol is explicitly proposal-only and has vote extensions
  disabled. Both features fail closed instead of silently enabling an unreviewed
  wire format.
- Bounded canonical wire codecs cover signed proposal/vote messages, validator
  proofs, proposed WAL values, Polka and round certificates, and liveness
  messages. Certificate signers are serialized in canonical address order;
  duplicates, invalid tags, excessive counts and trailing bytes are rejected.
- Bounded synchronization codecs cover status, range/value requests and
  responses. Historical values must be sequential, each commit certificate is
  bound to the canonical value identity and scope, disabled vote extensions
  fail closed, and encoded-length accounting matches the exact wire bytes.
- The proposal-only stream codec is scope-bound and accepts only bounded `Fin`
  frames; data parts remain uninhabited and fail closed.
- A bounded WAL-entry facade delegates the stable payload format to the pinned
  upstream implementation but validates its embedded length before decode.
  Tests write every safety-critical input to the upstream file WAL, flush,
  reopen and replay the exact sequence after process-local state is discarded.
- The standard upstream WAL actor is not used directly because its recovery
  path decodes an embedded payload length after the outer log has already
  accepted a much larger allocation bound. A drop-in actor implements the same
  public `WalRef`/`Msg` protocol, performs file I/O on a dedicated thread,
  preflights outer records before replay, and bounds each entry, aggregate bytes
  and entry count per height. Actor-level tests flush, stop, reopen and replay
  all safety-critical inputs exactly; worker panic reports a node safety failure.
- Commit certificates map into a bounded canonical Malachite evidence format.
  The adapter does not relabel Malachite precommit signatures as signatures over
  the laboratory checkpoint domain. An independent verifier reconstructs the
  exact scope-bound precommit and checks scope, value ID, height, canonical
  signers, every Ed25519 signature and weighted quorum before application commit.
- An actor-independent application bridge connects deterministic value build,
  remote validation, synchronized-value processing and decision callbacks to
  the framework-neutral application interface. It returns the same cached value
  for retries at one height and round, never admits rejected peer values as
  commit candidates, classifies peer faults separately from local transient
  failures, and advances the finalized tip only after verified evidence and a
  successful application commit.
- The pinned upstream `ProposalOnly` path currently marks a signature-valid
  embedded value valid without a host application-validation callback. A shared
  application-aware verifier closes that gap before engine admission: it checks
  the proposal signature first, re-executes the value through the same bridge,
  caches only accepted values, rejects peer-invalid values and escalates local
  execution failures instead of misclassifying the peer.
- A concrete ractor `HostMsg` adapter maps startup, round recovery, proposal
  build, decision/finalization, sync values and retained history to the bridge.
  It bounds history requests, independently verifies every historical finality
  certificate, disables vote extensions and requires an injected idempotent
  durable sink for equivocation evidence. Repeated decision/finalization
  callbacks after reply loss do not publish the application commit twice.
- A journal-first signer maps proposals, prevotes, precommits and validator
  proofs to a replaceable HSM/remote provider. Consensus slots include network,
  membership set, height, round and stage; the previous height-less slot model
  was corrected because it would have rejected honest round reuse at the next
  block. Reservations survive restart, conflicting payloads fail closed, and
  every provider signature is locally verified against the authenticated
  committee key before publication.
- A typed application stack derives the context, codecs, application-aware
  verifier and host from one scope and the exact same shared bridge. The first
  real lifecycle test composes the pinned Consensus actor, this host/verifier,
  journal-first signer and bounded WAL, finalizes one value and enters the next
  height. The test network is an in-memory actor used to isolate lifecycle
  wiring; authenticated multi-node transport and recovery faults remain open.
- Upstream libp2p permits unknown peers into its initial gossip mesh before
  validator-proof classification. Signatures still prevent forged votes, but
  that is insufficient for a permissioned validator plane. A `NetworkRef`
  proxy now forwards proposal, vote and liveness events only after proof
  verification and current-committee key membership. Committee updates revoke
  stale authorization and disconnect removes the session; sync remains a
  separate externally controlled channel with independent finality checks.
- A production-shaped transport constructor starts the pinned upstream
  QUIC/libp2p actor behind that proxy. Discovery is disabled, protocol and
  channel names are isolated by a canonical network namespace, frames use the
  platform consensus bound, peer scoring is enabled, and connections are
  persistent-peer-only unless a caller explicitly opts out. Validator identity
  construction also rejects a validator proof whose peer ID does not match the
  supplied libp2p key.
- A real three-node Linux integration test connects two committee validators
  and one non-committee validator. It verifies proof exchange, observes a
  committee vote across libp2p, and proves the outsider vote is not delivered
  to the subscribed consensus side. The test runs on Linux because the pinned
  upstream release unconditionally initializes system DNS; the macOS test host
  used for development exposes no nameserver through `/etc/resolv.conf`.
- A separate three-validator lifecycle composes the real Consensus, Host,
  journal-first signer, bounded WAL, permissioned QUIC/libp2p network and
  `ledger-consensus-application` on every node. The committee finalizes a real
  signed payment, fully stops, reopens the same LMDB/WAL/signer state with the
  same cryptographic identities and fresh network endpoints, then finalizes a
  second payment with the next nonce. All three independent stores recover the
  same two-block checkpoint, monetary state and retained payload history.
- The workspace MSRV is now Rust 1.88, matching the pinned upstream release.
- Vendor types do not enter the ledger, block-production, finality-proof or LMDB
  crates.

This is deliberately not a production consensus implementation yet. The next
slice must exercise crash recovery at individual durable boundaries,
same-endpoint process restart, partitions, catch-up and injected
storage/signing faults.

## Why Malachite first

- Rust implementation and library-shaped application boundary;
- permissive Apache-2.0 licensing;
- well-understood deterministic Tendermint BFT model;
- specification and model-checking artifacts;
- components for networking, synchronization and durable consensus history;
- close fit with a small permissioned committee and payment/stable-value use.

Its repository explicitly describes the software as alpha and not externally
audited. Therefore this decision authorizes an integration and benchmark, not a
production launch.

## Rejected shortcuts

- A new in-house BFT algorithm: unacceptable proof and audit burden.
- An unbounded proprietary fork of Malachite: creates permanent security and
  upgrade divergence without product value.
- Immediate full Polkadot SDK commitment: unresolved GPL and operational scope.
- Aptos Core reuse: the current license restricts competing blockchain use.
- Extracting Mysticeti or Agave consensus from their full nodes: excessive
  coupling to their transaction, execution and storage models.

## Production gate

The selected engine must pass the same seven-validator and multi-region tests,
including leader failure, two unavailable validators, asymmetric packet loss,
partitions, restart after durable vote, state catch-up, corrupt peers,
committee rotation and sustained payment load. It also requires:

- written license/SBOM approval;
- independent protocol and implementation audit;
- no conflicting finality or silent state divergence under fault injection;
- durable anti-double-sign recovery;
- independently verifiable finality certificates;
- reproducible p95/p99 finality and resource measurements;
- documented upgrade, rollback and incident procedures.

The detailed comparison and source list are in
[BLOCKCHAIN_TECHNOLOGY_RESEARCH_RU.md](../../docs/BLOCKCHAIN_TECHNOLOGY_RESEARCH_RU.md).
