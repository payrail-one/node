# ADR-0022: Bounded parallel transaction authorization

## Status

Accepted for the reference ledger runtime and block producer.

## Context

Authorization checks are independent for transactions in the same candidate
block, while monetary mutations are order-dependent. The initial runtime did
both tasks sequentially and repeated strict Ed25519 verification during proposal
construction, proposal reproduction and finalized-block verification. A local
pipeline profile showed those checks dominating LMDB commit time.

Parallel execution must not change consensus order, error precedence, receipts,
state bytes or roots. It also must not introduce a serializable "already valid"
flag that an untrusted peer could forge or persist beyond the verifier boundary.

## Decision

- `ledger-core` owns a process-local `VerifiedOperation` capability. Its fields
  are private, it has no wire or storage codec, and it is created only after the
  authoritative network-, role-, signer- and message-bound verification path.
- A runtime policy bounds workers to 64, defaults to the smaller of available
  parallelism and eight workers, and uses sequential verification below 64
  operations. Operators can select an explicit policy without affecting
  consensus-visible output.
- The runtime partitions a decoded bounded block into ordered chunks, checks
  their authorizations concurrently, then consumes every worker result in the
  original operation order.
- Monetary validation and mutation remain sequential. Therefore an earlier
  balance, nonce or policy failure still takes precedence over a later invalid
  signature, matching the original executor semantics.
- A verifier worker panic is joined and converted into a closed block failure.
  Every spawned worker is joined before returning, and no candidate state is
  published.
- `SignatureVerifier` exposes one ordered batch boundary with a conservative
  per-signature default. The Ed25519 adapter uses `ed25519-dalek`'s
  transcript-derived 128-bit batch coefficients and fixed-base acceleration
  tables. Before the batch equation it explicitly rejects weak public keys,
  non-decodable signature points and small-order signature points, preserving
  the security checks of the individual `verify_strict` path.
- A failed batch equation falls back to verification per operation. This keeps
  deterministic error positions and never accepts a partially valid batch.
- Proposal construction may reuse its in-process verified capabilities for the
  candidate execution pass. The completed wire payload is still decoded and
  verified by the ordinary block executor before it can be proposed.
- A complete side-effect-free transition produced while building or validating
  an exact consensus value can be retained in a bounded process-local cache.
  Finality verification happens before reuse, and value ID, parent, payload,
  block commitment and state commitment are checked again by `TailSyncSession`
  before the cached transition can reach storage. A cache miss executes through
  the ordinary finality-gated path.
- The configured `SignatureVerifier` remains a trusted cryptographic adapter.
  ADR-0059 now permits reuse of a process-local `VerifiedOperation` created by
  that same validator's ingress verification. The cache binds the complete
  signed envelope and network, has no wire/storage codec, and never substitutes
  for nonce, balance, fee, expiry or other ordered block validation.
- The worker policy and ordered verification implementation live in the shared
  `transaction-verification-core` crate. Runtime execution and local batch
  ingress therefore use one implementation rather than parallel copies.

## Consequences

Equivalence tests prove that sequential and bounded-parallel modes produce
byte-identical transitions and receipts. Tests also cover deterministic error
precedence, wrong-network rejection and fail-closed worker failure.

The current canonical single-validator block-runtime median is 263,223.74
transfers/s for 12,288-operation blocks. The complete three-validator
Malachite/QUIC/LMDB local-host median is 36,787.04 finalized transfers/s. These
are local engineering results, not production capacity; separate-host and
multi-region gates remain open.

The added batch dependencies retain permissive licenses: `ed25519-dalek` and
`curve25519-dalek` are BSD-3-Clause, while `strobe-rs` and `keccak` are
MIT/Apache-2.0 or Apache-2.0/MIT.
