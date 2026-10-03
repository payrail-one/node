# ADR-0024: Versioned authenticated normalized ledger state

## Status

Accepted for the reference authenticated-state boundary. ADR-0026 adds an
explicit height-gated checkpoint-root migration; production activation still
requires signed network configuration and multi-validator compatibility tests.

## Context

Hashing the complete canonical state image is deterministic, but it requires
work proportional to the whole ledger for every block and cannot produce a
compact proof for one balance, asset or account policy. A high-throughput
payment network needs incremental state commitments, retained historical roots
and independently verifiable membership and non-membership proofs.

The normalized runtime and LMDB schema divide live state into network identity,
registry authority, assets, balances, nonces, one operation-sequence row and
account statuses. The authenticated structure must preserve those boundaries,
remain deterministic across insertion order and start safely from a verified
snapshot at an arbitrary chain height.

## Decision

- Use a Jellyfish Merkle Tree through pinned `jmt = 0.12.0`, based on the Diem
  design, for the reference versioned authenticated-state implementation.
- Disable the crate's default ICS23 and SHA features. The platform supplies a
  single SHA-256 implementation with explicit
  `payment.authenticated-state.jmt.v1` and
  `payment.authenticated-state.key.v1` domains.
- Hash the stable one-byte ledger namespace together with the bounded raw row
  key. Namespace values are consensus-visible and must not be renumbered.
- Authenticate network identity and registry authority as singleton rows, then
  authenticate assets, balances, nonces, the operation sequence and account
  statuses in separate namespaces.
- Enforce non-empty keys, a 64-byte key limit, a 512-byte value limit, at most
  1,000,000 changes per tree version, duplicate-key rejection and sequential
  versions before changing the store.
- Apply the complete upstream tree update to a cloned in-memory store and swap
  only after all node/value conflict checks pass. A failed version leaves the
  previous root and retained nodes unchanged.
- Map an arbitrary verified snapshot height to local tree version zero. Chain
  height `base + n` maps to tree version `n`; no synthetic empty tree versions
  are created.
- Derive updates by a deterministic merge-diff of canonical normalized rows.
  Incremental roots must equal roots produced by a complete rebuild.
- Bind proof objects to namespace, raw key and returned value. Both membership
  and non-membership proofs are supported, including at retained historical
  heights.
- Reject network or registry-authority changes in the incremental runtime view
  until an explicit, separately governed state transition exists.

## Dependency and supply-chain review

`jmt 0.12.0` is Apache-2.0 licensed. Its newly introduced transitive packages
report permissive Apache-2.0, MIT, BSD-2-Clause or Unlicense-compatible terms in
Cargo metadata. Versions are locked in `Cargo.lock`; direct dependencies are
pinned and default features are disabled. This review is an engineering gate,
not a substitute for the future legal SBOM review, external cryptographic
review, fuzzing and long-running property tests.

The implementation deliberately uses an established authenticated-tree design
instead of inventing a Merkle algorithm. Primary references:

- [Jellyfish Merkle Tree paper](https://developers.diem.com/docs/technical-papers/jellyfish-merkle-tree-paper/)
- [Penumbra JMT repository](https://github.com/penumbra-zone/jmt)
- [`jmt` 0.12.0 API documentation](https://docs.rs/jmt/0.12.0/jmt/)

## Consequences

The reference tree has stable order-independent roots, incremental set/delete
updates and historical proofs. Runtime tests prove that a payment-derived row
delta produces the same root as a full rebuild and that a snapshot at height
100 correctly retains proofs for heights 100 and 101.

After the explicit ADR-0029 bounded-state migration, the version-two
normalized-ledger fixture is pinned to root
`53ac4eed0f2982bf265f70130aeb7e777df4c898da5776793a32994e58e0018f`.
Any change to namespaces, row codecs, key derivation or tree hashing must either
preserve this vector or be treated as an explicit state-format migration.

Historical nodes are currently retained without pruning. A production pruning,
archive and proof-retention policy remains required. ADR-0025 adds persistent
LMDB nodes, versioned values and atomic tree/ledger/block/cursor publication.
ADR-0026 allows a configured activation height to make this root authoritative;
the backward-compatible default still retains the full-state commitment.
ADR-0029 removes append-only receipts from live authenticated state and retains
their source payloads in finalized block storage instead.
