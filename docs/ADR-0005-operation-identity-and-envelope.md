# ADR-0005: Operation identity and bounded binary envelope

Status: accepted for the reference protocol; receipt-in-snapshot and repeated-ID
restoration clauses are superseded by ADR-0029

## Context

Wallets, gateways and nodes need one unambiguous representation of a signed
operation. Idempotency keys identify client retries but do not prove that two
requests contain the same payment. Receipts need a content-derived identifier,
and untrusted wire input must be decoded without unbounded allocations,
ambiguous trailing data or JSON number conversion.

## Decision

1. Every authorized operation has canonical bytes containing an explicit
   operation discriminant and all monetary fields. These are the same operation
   bytes embedded in signer-role authorization messages.
2. `OperationId` is SHA-256 over a distinct neutral domain separator followed
   by canonical operation bytes. Signatures are excluded: the identifier names
   the payment intent rather than one cryptographic witness.
3. Successful receipts and snapshots store `OperationId`. Snapshot restoration
   rejects repeated operation IDs as an invalid replay history.
4. `transaction-protocol` provides one binary signed-operation envelope for internal
   wallet/API/node boundaries. It contains a fixed domain, canonical operation,
   sender authorization and exactly the sponsor authorization required by the
   operation variant.
5. The decoder rejects input above 8 KiB before parsing, batch counts above the
   ledger maximum before allocation, unknown variants, invalid authorization
   flags, mismatched authorization counts, truncation and trailing bytes.
6. Integers are fixed-width big-endian values. Batch item count is an unsigned
   32-bit value and batch order is preserved.

## Dependency review

The ledger pins RustCrypto `sha2` exactly to `0.11.0` with default features
disabled. It is implemented in Rust and licensed under Apache-2.0 or MIT. SHA-256
is used only for public operation identity; signatures continue to use the
separate strict verification adapter.

## Security consequences

- A client can compare the `OperationId` in an existing receipt with the
  operation it intended when an idempotency key has already been consumed.
- Changing the operation variant, network, account, asset, amount, fee, nonce,
  sponsor, batch order or idempotency key produces a different identifier.
- Equivalent valid envelopes have one encoding. Extra bytes are not silently
  ignored, preventing different components from signing and executing different
  interpretations.
- The 8 KiB bound is defense in depth, not a replacement for HTTP/gRPC body
  limits, authentication, rate limiting or admission control.
- Canonical bytes are consensus-sensitive. Future incompatible schemas require
  a distinct domain and an explicit migration rather than reinterpretation.
- `OperationId` is not proof of finality. A finalized block reference and
  consensus proof remain necessary for settlement evidence.

## Rejected alternatives

- Treating the client idempotency key as a transaction hash: it is chosen by the
  caller and does not commit to transaction content.
- Hashing signatures into operation identity: complicates signer migration and
  makes one payment intent acquire different identifiers.
- Accepting JSON as the canonical signed or hashed form: object ordering,
  duplicate keys and number handling differ across implementations.
- Allocating a declared batch before checking its count: permits a trivial
  memory-exhaustion path at untrusted ingress.
