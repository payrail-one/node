# ADR-0025: Atomic LMDB persistence for authenticated ledger state

## Status

Accepted for the reference finalized-state adapter. ADR-0026 defines the
separate, height-gated promotion of the authenticated root to the consensus
checkpoint.

## Context

ADR-0024 introduced deterministic incremental ledger roots and historical
proofs, but its first store was process-local. Rebuilding a tree after every
restart would lose historical proof availability and would not prove that the
authenticated root advanced atomically with normalized balances, the canonical
state image, finalized block record and cursor.

The persistent adapter must reuse the single authoritative hashing and mutation
preparation path, fail closed on corrupt records, preserve older versions and
remain recoverable from a verified snapshot at an arbitrary chain height.

## Decision

- `authenticated-state-core::backend` owns mutation validation, key hashing,
  JMT update preparation, root reads and proof construction. LMDB does not
  reimplement those consensus rules.
- LMDB stores immutable JMT nodes, versioned values, stale-node indices and a
  current live-leaf index in four dedicated databases.
- Node keys and nodes use the pinned JMT/Borsh encoding. Value keys are
  `key_hash || big_endian_version`; values have an explicit present/tombstone
  marker and remain bounded to the authenticated-state value limit.
- A checksummed authenticated cursor binds base chain height, latest chain
  height, local tree version and root. The decoder verifies
  `tree_version == latest_height - base_height`.
- Snapshot initialization prepares tree version zero and publishes its nodes,
  normalized rows, canonical state image, base/current cursors and authenticated
  cursor in one LMDB write transaction.
- Every finalized ledger block reads the previous normalized rows and persisted
  JMT version, prepares only the deterministic row delta, then publishes tree
  nodes/values, normalized row changes, state image, block audit record, full
  block payload, authenticated cursor and finalized cursor in one LMDB write
  transaction.
- Complete canonical state images are bounded to the verified recovery base and
  current finalized state. When current advances, a non-base predecessor image
  is deleted in the same transaction that publishes its successor. Historical
  block records, full payloads and authenticated proof data remain
  independently retained.
- Generic byte-state mode cannot advance a store initialized in ledger mode;
  callers must use the ledger commit path so authenticated state cannot lag the
  finalized cursor.
- Existing node/value keys are immutable. An unexpected collision aborts the
  transaction as `TreeConflict` rather than overwriting authenticated history.
- Reopen validates the block chain, canonical state image, normalized ledger
  invariants, a complete rebuilt authenticated root and the persisted JMT root.
- Historical membership and non-membership proofs are served from persisted
  nodes and values and verified against the retained root for the requested
  chain height.

## Consequences

The restart test commits a real Ed25519-authorized payment, closes the LMDB
environment, reopens it, commits another signed block using persisted JMT
history, and verifies balances, nonces, tree version and proofs at both the base
and latest heights. The existing LMDB map-full test continues to prove that a
failed write transaction publishes neither state, block nor cursor.
Another restart test advances three blocks and proves that complete state-image
retention remains bounded to base plus current while the full block chain and
its hash-bound payloads stay valid after reopen. The map-full test proves the
payload cannot become visible without its state, block record and cursor.

The storage format is internal and version-sensitive: changing JMT, Borsh,
namespace values or record encoding requires an explicit migration and stable
vectors. Historical JMT nodes, values and stale indices are retained; their
production pruning, archive retention and proof-availability policy remain
open. Complete canonical state images do not grow per block. A checksummed
height-gated policy can now promote the JMT root, while the default remains the
legacy full-state commitment until a network explicitly schedules activation.
ADR-0029 defines the bounded live-state and receipt-index boundary; finalized
payload retention remains append-only until an archive/checkpoint policy is
approved.
