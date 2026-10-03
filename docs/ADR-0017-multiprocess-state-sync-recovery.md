# ADR-0017: authenticated multi-process state-sync recovery

## Status

Accepted as a local integration harness. It is not a production snapshot
service, remote multi-region test, or complete node recovery implementation.

## Context

A fast catch-up path must not trust a provider merely because it completed an
mTLS handshake. The receiving node must independently verify checkpoint
finality, provider admission context, the manifest, every chunk, durable storage
and the applied state root. Recovery also needs evidence that a corrupt provider
cannot poison progress and that another provider can safely retry the snapshot.

## Decision

- The existing validator process harness also starts isolated snapshot-provider
  processes with unique TLS certificates.
- Voting and state-sync reuse one implementation of process lifecycle, bounded
  TLS material loading, TLS 1.3 mutual authentication, ALPN and certificate
  fingerprint pinning.
- A provider sends a bounded canonical manifest, a real five-of-seven Ed25519
  finality certificate and bounded chunk frames.
- The receiver starts `StateSyncSession` only after the shared strict finality
  verifier accepts the checkpoint and proof.
- Each TLS peer fingerprint is bound to the `SyncProvider` admission passed to
  the domain session. Network, membership epoch, role and protocol must match.
- Verified chunks are published through `FileSnapshotStore` before progress
  advances. The applied state root is computed from the durably re-read chunks.
- After the first provider, all in-memory session and store objects are dropped.
  A fresh session re-verifies finality, reopens the store and reconstructs
  progress through the shared `VerifiedChunkReader` boundary before retrying.
- The first provider deliberately corrupts one same-length chunk. Its hash
  mismatch is rejected without progress; a second authenticated provider sends
  the canonical snapshot, with prior chunks handled as idempotent duplicates.
- Shared security-sensitive helpers are authoritative modules. New harness paths
  must reuse them rather than copy TLS, identity or child-process code.

## Limits

The catch-up receiver currently runs in the controller process and providers
serve a fixed fixture once. Membership admissions are controlled harness
fixtures rather than records fetched from governance state. The harness does not
yet cover a kill during an individual filesystem write, large snapshots,
bandwidth shaping, remote hosts or production runtime application of tail blocks. Reported
latency is a local integration measurement, not a production recovery SLO.
