# ADR-0028: Two-phase incremental authenticated execution

## Status

Accepted for the reference runtime, block producer, tail verifier and LMDB
adapter. Remote-validator equivalence, restart orchestration and production
retention policy remain required before deployment.

## Context

ADR-0026 made the normalized Jellyfish Merkle Tree root consensus-visible. The
stateless executor correctly rebuilt the complete tree when reproducing a
proposal and again when verifying a finalized block. LMDB then prepared the
same row delta against its persistent tree. This was safe but repeated work on
the payment hot path and made cost grow with retained ledger state.

An optimization cannot let proposal construction mutate finalized state, trust
a root supplied by storage, publish an update before finality, or leave the
ledger rows and authenticated tree at different heights.

## Decision

- `AuthenticatedStateTree::prepare` creates an opaque update against one exact
  prior version and root without mutation. `commit` consumes it only if that
  predecessor is still current. Competing prepared updates cannot both commit.
- The in-memory tree store validates every node, value-version and stale-node
  conflict before applying any mutation. It no longer clones the full retained
  tree to provide failure atomicity.
- `LedgerAuthenticatedState` applies the same prepare/commit contract to
  canonical normalized rows, chain height and state root. Preparation validates
  the full snapshot and network/registry identity before calculating a bounded
  sorted row delta.
- `LedgerBlockExecutor::execute_incremental` binds the encoded predecessor to
  the exact authenticated view, executes normal signed operations, and prepares
  the next row delta. The stateless execution method remains as an independent
  fallback and equivalence oracle.
- Proposal selection has an incremental variant. A proposer retains its already
  executed transition and opaque update until the proposed payload is finalized;
  it does not rebuild the tree merely to reproduce its own result.
- `TailSyncSession::verify_next_with` checks network, height, parent, bounds and
  finality before invoking an auxiliary transition preparer. It returns that
  opaque auxiliary value only after the computed commitments match the finalized
  checkpoint. Validators without a matching local proposal use incremental
  execution as the fallback.
- Durable LMDB publication remains authoritative. It prepares the persistent
  row delta and verifies its JMT root inside the same write transaction that
  publishes normalized rows, canonical state, block record and finalized
  cursor. It no longer performs a redundant full-tree rebuild first.
- The process must durably commit LMDB before publishing the process-local
  prepared view. If the process stops between those steps, restart reconstructs
  the in-memory view from the durable canonical snapshot; it never rolls LMDB
  back to process memory.
- Legacy `CanonicalStateV1` checkpoints still receive a direct canonical-state
  hash check. The LMDB incremental-root check is used only when
  `AuthenticatedStateV1` is active.

## Security properties

- Preparation is side-effect free and all fallible validation precedes mutation.
- A prepared capability is bound to its predecessor height, root and tree
  version; stale or competing commits fail closed.
- Finality is checked before potentially expensive externally triggered
  transition preparation.
- The finalized checkpoint must match the runtime transition and the
  independently prepared persistent LMDB root.
- A wrong authenticated checkpoint root aborts and rolls back the complete LMDB
  transaction, including state, rows, tree nodes, block record and cursor.
- Incremental and complete-rebuild paths must remain byte-identical and are
  tested across the commitment activation height.

## Evidence

Tests cover side-effect-free preparation, competing updates, exact predecessor
binding, activation from canonical to authenticated roots, proposal equivalence,
finality-before-preparation, and LMDB rollback on an authenticated-root mismatch.
The multi-process recovery harness also discards its process-local authenticated
update and tail cursor immediately after a successful LMDB commit, reopens the
environment, rebuilds the live view from the durable snapshot and continues two
more incrementally executed finalized blocks.

Three comparable local 1,000-transfer runs with 500 transfers per block produced
9,250.28, 10,759.95 and 9,528.65 transfers/s. The median was **9,528.65
transfers/s**, 24.49% above the previous 7,653.81 authoritative-root median.
The measured path includes client signing, bounded admission, proposal execution,
authenticated-root preparation and atomic LMDB persistence, but excludes network
finality, WAN and HSM latency. It is not a production TPS or checkout-latency
claim.

## Consequences

Authenticated commitment work now scales with the changed normalized rows in
the runtime and persistent store rather than requiring repeated full-tree
rebuilds. The proposal scheduler still applies candidates and then reproduces
the selected block through the authoritative executor; safe removal of that
remaining duplicate execution needs a separately reviewed capability boundary.

ADR-0029 additionally removes append-only receipt history from the live
authenticated view. Runtime receipts remain deterministic block-local results,
while LMDB atomically retains each complete finalized payload for indexing and
recovery.

The in-memory reference tree and persistent LMDB tree remain separate
implementations so storage cannot dictate consensus. Historical pruning,
remote multi-validator compatibility, abrupt OS-process termination during the
same publication window, and real consensus/network benchmarks remain open.
