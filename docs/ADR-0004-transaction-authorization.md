# ADR-0004: Canonical transaction authorization

Status: accepted for the reference ledger; final runtime cryptography remains a
Phase 2 decision gate

## Context

A payment signature must cover every field that can alter monetary effects. A
sponsor must consent independently, signatures from one role must not be
reusable in another role, and a transaction signed for one network must not be
valid in another. The domain ledger must remain testable without depending on a
wallet, HSM, API framework or one cryptographic implementation.

## Decision

1. The ledger defines a small `SignatureVerifier` interface. Cryptographic
   implementations live in separate adapter crates.
2. `AuthorizedOperation` distinguishes transfer, batch, sponsored transfer and
   sponsored batch. The variant is part of the signed message, so an ordinary
   payment cannot be converted into a sponsored one after signing.
3. Canonical authorization messages contain a fixed neutral domain separator,
   signer-role and operation discriminants, network identifier, idempotency key,
   asset, sender, recipients, integer amounts, fee, nonce and—when present—the
   fee payer. Integers use fixed-width big-endian encoding and batch order is
   significant.
4. Sponsored operations require two signatures over the same complete
   operation, but with distinct sender and fee-payer role discriminants.
5. The reference crypto adapter treats a 32-byte account identifier as an
   Ed25519 public key and uses strict verification. Invalid encodings, weak keys
   and non-canonical signatures fail before ledger mutation.
6. A wrong network is rejected before signature verification. Authorization
   structure and all signatures are verified before monetary validation and
   state mutation.
7. Lower-level `transfer` methods remain a trusted-runtime boundary for callers
   that already possess an authenticated origin. Untrusted node/API ingress uses
   `submit_signed`.

## Dependency review

The adapter pins `ed25519-dalek` exactly to `3.0.0`, compatible with the
workspace Rust 1.85 baseline and licensed under Apache-2.0 or MIT. Default,
legacy-compatibility, hazmat, random-generation and batch-verification features
are disabled. Production code performs verification only and never accepts or
stores private keys. Deterministic test keys enable the upstream `zeroize`
feature in test builds.

`verify_strict` is required because ordinary Ed25519 verification may accept
weak public keys. Batch verification is deliberately not enabled because it
does not perform the same weak-key check.

## Security consequences

- Any change to a recipient, amount, fee, nonce, asset, network, idempotency key,
  batch ordering, sponsor or operation type invalidates the signature.
- A sender signature cannot be copied into the fee-payer slot because signer
  role is included in the message.
- Canonical authorization bytes are a consensus-sensitive format. Any future
  encoding change requires a new domain separator and explicit migration; silent
  reinterpretation is forbidden.
- Ed25519 here is an adapter choice, not a commitment for validator consensus or
  the final account scheme. Polkadot SDK compatibility, HSM support and audited
  wallet libraries remain inputs to the architecture bake-off.
- Signature validity does not replace nonce, idempotency, balance, backing,
  account-policy or network checks; all existing ledger rules still apply.

## Rejected alternatives

- Serializing an arbitrary JSON object for signing: field order, number handling
  and parser differences can create ambiguous payloads.
- Signing only a transaction hash supplied by the API: the node must construct
  the canonical preimage itself and must not trust an unbound client hash.
- Reusing one signature for sender and sponsor roles: it permits unintended fee
  delegation and weakens consent evidence.
- Embedding key generation or private-key custody in the ledger: violates the
  isolation required for wallets, secure elements and HSMs.
