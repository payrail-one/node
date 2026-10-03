# ADR-0019: LMDB adapter for finalized node state

## Status

Accepted for the local node-state adapter. The ledger key schema remains a
prototype until the production runtime is selected and benchmarked.

## Context

Tail synchronization must never expose a new finalized cursor with an old
ledger state, or a new state without the block that produced it. Publishing
separate files cannot provide one transaction across these records. The project
also needs fast local reads and restart recovery without operating a separate
database service on every validator.

## Decision

- Use original LMDB through pinned `heed 0.22.1` for the local finalized-state
  adapter. Default features and Serde formats are disabled.
- Keep the storage port separate from consensus and transition verification.
  LMDB is an adapter, not part of the consensus algorithm.
- Use separate typed databases for metadata, finalized block records, complete
  finalized payloads, content-addressed state images, assets, balances, the
  operation sequence, nonces and non-default account policies.
- Initialize the verified snapshot, normalized ledger rows and cursor together.
  Each subsequent write transaction publishes row-level ledger changes, the
  canonical state image, block audit record, complete payload and new finalized
  cursor atomically.
- Encode values canonically with explicit domains, lengths and SHA-256 storage
  checksums. Consensus state roots remain runtime-defined and are verified by
  `tail-sync-core` before the LMDB transaction begins. The ledger-aware entry
  point independently decodes the canonical state and verifies the root again.
- Bind every environment to one network and reject stale, conflicting or
  cross-network commits without mutation. Exact retries are idempotent.
- Keep normal LMDB locking and durability. `NO_SYNC`, `NO_META_SYNC`, `WRITEMAP`
  and `NO_LOCK` are prohibited for validator state.
- LMDB directories must be on a local filesystem controlled exclusively by the
  node. Live `data.mdb` files must not be edited, copied piecemeal or placed on
  NFS/network volumes.
- Keep transactions short. `MapFull` is a distinct operational error; map-size
  growth requires an explicit controlled operation and monitoring.

## Unsafe boundary

`heed::EnvOpenOptions::open` is unsafe because LMDB uses memory mapping. One
localized unsafe block is allowed in `tail-state-store-lmdb` under these
preconditions:

1. the path is canonical, application-owned, local and not a symlink;
2. files inside the live environment are never modified outside LMDB;
3. LMDB locking remains enabled;
4. mapped references never escape transaction lifetimes;
5. no unsafe performance/durability flags are accepted by configuration.

All other workspace crates continue to forbid unsafe code.

## Consequences and limits

The adapter retains the recovery-base and current canonical state images for
snapshot export while the live ledger is normalized into dedicated tables.
Updates use a sorted row diff, so unchanged balances and assets are not
rewritten. On restart, the normalized rows are reconstructed through the ledger
invariant validator, re-encoded and checked against both the finalized root and
canonical image. Complete post-base block payloads are retained separately and
checked against their immutable audit hashes so receipt history can be indexed
without putting it back into live consensus state; see ADR-0029.

The schema is still pre-production: payload/archive and authenticated-proof
pruning, receipt-index checkpoints, online map resizing, backup coordination
and schema migration need benchmarked policies before a persistent public
testnet.

LMDB has one writer per environment. This matches deterministic block
application but must be benchmarked with realistic state, block size, pruning,
backup and restore workloads. LMDB replication is not used; validator
replication comes from the consensus protocol and verified state sync.

`heed` is MIT licensed. LMDB uses the OpenLDAP Public License 2.8, whose notice
and verbatim-license redistribution requirements must be included in the SBOM
and distribution review.
