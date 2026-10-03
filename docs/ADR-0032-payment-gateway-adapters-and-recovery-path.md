# ADR-0032: Payment gateway adapters and recoverable finalized path

## Status

Accepted for the Rust reference implementation.

## Context

ADR-0031 defined safe payment orchestration and journal-first idempotency behind
ports. Those invariants still needed concrete adapters to prove that a request
could pass through the existing cryptography, bounded mempool, deterministic
runtime, atomic LMDB ledger, archived payload, rebuilt receipt index and gateway
reconciliation without substituting test-only transaction formats.

The gateway nonce lookup must also stay off the full-snapshot reconstruction
path. Rebuilding and revalidating every account and balance for each checkout
would make the retail latency target unattainable as state grows.

## Decision

Add `payment-gateway-adapters` with these replaceable implementations:

- `FinalizedLedgerNonceSource` reads the account nonce from the current
  authenticated normalized state;
- `StaticPaymentPolicy` enforces an explicit asset allowlist, maximum amount,
  maximum fee and sponsorship switch before nonce allocation;
- `LocalEd25519KeyringSigner` creates sender and optional fee-payer role-bound
  signatures for local development and recovery tests;
- `Ed25519PaymentVerifier` reuses the shared strict ingress verification path;
- `LocalMempoolPublisher` serializes access to the existing bounded gossip pool,
  performs canonical encoding and distinguishes new from already-known input.

The tail LMDB store exposes `current_nonce(account)`. It reads the nonce leaf at
the current authenticated tree version, verifies the membership or
non-membership proof against the persisted root and only then decodes the
fixed-width value. A missing leaf means nonce zero; an explicitly stored zero or
malformed value is corruption. This is an `O(log n)` authenticated lookup and
does not reconstruct the complete ledger snapshot.

The integration test drives a real payment through canonical request hashing,
Ed25519 signing, strict verification, journal-before-publish, the bounded pool,
deterministic block production, finality verification and atomic normalized LMDB
commit. It then closes the live components, reopens the ledger and payment
journal, rebuilds the independent receipt index from the archived signed block,
and reconciles the request to `Finalized`. Final balances, fee, nonce and
checkpoint are checked after restart.

## Security boundaries

- The local keyring is explicitly not a production custody solution. Production
  keys belong in an HSM, MPC or threshold signing service implementing the same
  port, with authorization and audit controls.
- The static policy is a deterministic technical guard, not KYC, AML, sanctions,
  fraud or jurisdiction policy.
- The local pool adapter is process-local. A production gateway must publish to
  authenticated validator/sentry infrastructure and preserve the same exact-byte
  retry semantics.
- A positive pool acknowledgement is `Published`, not `Finalized`. Only indexed
  receipt evidence can complete a payment.
- The integration proof uses a deterministic test finality verifier. It proves
  component composition and recovery, not multi-region BFT latency or safety.

## Consequences

The repository now has an executable merchant-payment path using the same signed
operation and persistence formats as the ledger. Process restart between ledger
finalization and receipt indexing no longer loses the ability to determine the
payment outcome.

Production HSM, remote authenticated publishing, public HTTP/gRPC schemas,
identity/compliance policy, background reconciliation workers and multi-instance
gateway coordination remain open work.
