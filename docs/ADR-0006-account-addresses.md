# ADR-0006: Network-scoped account addresses

Status: accepted for the reference wallet boundary; production identifiers and
key derivation remain decision gates

## Context

Raw 32-byte account identifiers are suitable for ledger state but unsafe and
unfriendly for manual transfer, QR codes, exchanges and external wallets. An
address must detect transcription errors, bind visibly to one configured
network and remain independent of a provisional product name. Choosing a wallet
coin type or derivation path before the final key scheme and registry work would
create avoidable incompatibility.

## Decision

1. Consensus continues to use raw `AccountId` and `NetworkId`. A human-readable
   address is a wallet/API presentation format, not a new ledger identity.
2. `account-address` encodes a one-byte account-address type discriminator plus
   the 32-byte account ID with Bech32m.
3. Every network is configured with a unique lowercase HRP. A codec is bound to
   both that HRP and the full `NetworkId`; cross-network decoding fails.
4. Only lowercase canonical strings are accepted. The parser rejects legacy
   Bech32 checksums, malformed checksums, unknown discriminators, incorrect
   payload lengths and strings longer than 90 characters.
5. No product HRP is hardcoded. `main` and `test` are test fixtures only. Final
   prefixes require naming clearance, collision review and registry updates.
6. No production derivation path or SLIP-0044 coin type is assigned yet. The
   coin type must be registered rather than invented, and the derivation profile
   must follow the final curve decision.

## Standards and dependency review

Bech32m is selected for its compact QR-friendly alphabet, case-insensitive
visual form and checksum. BIP-350 introduced the Bech32m checksum constant to
address the insertion weakness identified in the original Bech32 construction.

The crate pins `bech32` exactly to `0.12.0`, disables default features and
enables only allocation support. It has no transitive dependencies, supports a
Rust baseline older than this workspace and is MIT licensed. The platform adds
its own 90-character limit because the crate's generic encoder intentionally
does not enforce that limit for every application.

SLIP-0044 defines registered hardened coin types. If Ed25519 remains an account
key option, SLIP-0010 permits hardened child derivation only and does not permit
public-parent-to-public-child derivation. This affects watch-only wallets and
must be resolved in the wallet architecture rather than hidden in an address
codec.

## Security consequences

- Distinct HRPs make network confusion visible and machine-rejectable, while
  signed operations independently bind the complete 32-byte network ID.
- Checksums catch common transcription errors but do not authenticate a payee.
  Wallets still need verified contacts, confirmation screens and anti-phishing
  controls.
- A trusted network configuration maps HRP to `NetworkId`; accepting arbitrary
  HRPs supplied by a request would defeat the boundary.
- The type discriminator permits a future explicitly migrated account encoding
  without reinterpreting existing strings.
- External-wallet onboarding will require matching address, derivation,
  signing and transaction vectors after the cryptography decision is final.

## Rejected alternatives

- Hexadecimal account IDs: long, easy to mistype and not network-specific.
- Base58 without a network prefix: weak human network separation.
- Hardcoding a provisional product prefix: leaks an unsettled name into APIs,
  QR codes, databases and partner integrations.
- Reusing one HRP across production and test networks: makes accidental
  cross-environment transfers harder to detect.
- Assigning an arbitrary coin type now: conflicts with the registry process and
  can strand early wallet integrations on an unofficial path.
