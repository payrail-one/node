# ADR-0009: Verified parallel state sync

Status: accepted for the reference sync boundary; node storage and consensus
proof adapters remain stack-specific

## Context

A new or recovering node must catch up much faster than replaying the complete
history, but accepting a database archive from one fast peer would allow that
peer to inject false balances, membership or validator state. Sync must permit
parallel transfer without making download order or provider trust part of
consensus.

## Decision

1. A snapshot manifest binds `NetworkId`, protocol digest, snapshot-format
   identifier, finalized block height/hash, state root, validator-set hash,
   total byte size and an ordered list of chunk offsets, lengths and hashes.
2. Manifest and chunk identifiers use domain-separated SHA-256. Chunks are
   content-verified independently and may arrive in any order from multiple
   currently admitted `SyncProvider` peers.
3. A session starts only after a stack-specific `FinalityProofVerifier` accepts
   the checkpoint proof. The opaque proof is bounded to 1 MiB before adapter
   processing.
4. Providers must match the manifest network and current membership epoch and
   must have authenticated specifically for the `SyncProvider` role.
5. Chunk layout is canonical and contiguous. Empty chunks, gaps, overlaps,
   indices out of order, chunks over 8 MiB, more than one million chunks and
   snapshots over 16 TiB are rejected.
6. Exact chunk retries are idempotent. Wrong length or hash never advances
   progress.
7. Download completion is not node readiness. After applying all chunks, the
   node independently computes state root and compares it with the finalized
   checkpoint. Only then may it tail-sync blocks and become authoritative.

## Integration requirements

`state-sync-core` does not own files, databases or sockets. It calls a narrow
verified-chunk persistence boundary and advances in-memory progress only after
that write succeeds. The filesystem reference adapter stores chunks
content-addressably with a durable temporary write and atomic publication. A
crash after publication but before the in-memory update is safe because the
same write is idempotent. The production node must:

- reconstruct a new session from revalidated durable chunks after restart; it
  must not trust a stale in-memory bitmap;
- enforce disk quotas and reserve space before download;
- never deserialize or apply partially verified chunks directly into live state;
- apply into a new database namespace and atomically switch only after root
  verification;
- delete temporary data safely after success or expiry;
- revalidate membership epoch for long-running sessions;
- fetch from several providers and penalize corrupt responses without trusting
  majority byte content over the finalized state root.

The stack-neutral reference quorum verifier and its explicit limitations are
defined in `ADR-0012-finality-proof-boundary.md`. Production Polkadot SDK
integration must use a GRANDPA justification/authority-transition adapter at
this boundary.

## Performance consequences

- 8 MiB chunks are large enough for efficient sequential I/O and small enough
  for bounded retries and parallel scheduling. The value is a benchmark input,
  not a claim of optimality for every deployment.
- Manifest verification is linear in chunk count; actual hashing and download
  can be parallelized by the node implementation.
- Metrics separate manifest/finality verification, download, chunk hashing,
  state apply, root calculation and tail catch-up.

## Rejected alternatives

- Trusting a configured peer's archive without root verification: membership
  limits who may connect but does not guarantee an uncompromised machine.
- One unbounded snapshot file: poor retry granularity and trivial memory/disk
  exhaustion risk.
- Marking a node ready when download finishes: bytes may be corrupt or apply to
  a different state root.
- Choosing the snapshot advertised by the highest peer: height claims are not
  finality proofs.
