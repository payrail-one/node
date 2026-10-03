# Security model

## Scope

The current public environment is a development network. It is suitable for
integration and verification work with test assets, not custody or real-value
payment settlement. The declared finality mode is part of the security
contract; `single-node-devnet` must not be presented as BFT finality.

## Assets to protect

- wallet and validator private keys;
- API, deployment and provider credentials;
- ledger supply, balances and nonces;
- canonical authorization and operation identity;
- finalized payload history and state commitments;
- merchant tenant isolation and checkout ownership;
- consensus signing history and validator-set trust anchors;
- availability under bounded adversarial input.

## Trust boundaries

### Browser

Wallet signing happens client-side. Encrypted vaults use browser cryptography;
plaintext private keys must not cross into SSR, Workers, analytics, logs,
screenshots or API payloads. Browser UI is not authoritative for balances,
fees, finality or receipt outcome.

### Public HTTP

Gateways must enforce body, concurrency, rate and timeout limits before
decoding expensive input. A signed envelope proves only the bound operation; it
does not authenticate an unrelated merchant or administrative action.

### Consensus and peers

Transport identity, consensus identity and account authorization are separate
key roles. Membership, network, epoch, protocol, challenge and channel binding
are checked before peer traffic enters consensus. Consensus signing is
journal-first and rejects conflicting votes after restart.

### Persistence

Authoritative ledger state is committed atomically. Receipt indexes and UI
views are derived and recoverable from verified history. Corruption,
configuration mismatch or impossible state fails closed rather than resetting
or silently repairing value-bearing state.

### External settlement

R1/C1 and other external assets are separate trust domains. Remote JSON is not
finality merely because the same server provides a validator set and proof.
Pinned chain/genesis/epoch/validator commitments and successful-execution proof
verification are required.

## Required properties

- domain separation for signatures, identifiers and commitments;
- canonical, bounded decoding with trailing-data rejection;
- checked arithmetic and atomic state change;
- exact retry semantics after ambiguous publication;
- replay protection by finalized sender nonce;
- independent finality and receipt verification;
- bounded memory, disk, peer and identity accounting;
- explicit network/configuration binding of durable stores;
- least-privilege processes and loopback defaults.

## Secrets

Never commit real secrets, private keys, certificates with private material,
deployment state or private infrastructure addresses. Examples use placeholders
only. Secret values must not be passed as command-line arguments when they can
appear in process listings; use protected files, environment injection or an
approved signer boundary.

## Vulnerability reporting

Do not open a public issue for a suspected vulnerability. Follow the repository
[security policy](../SECURITY.md). Include the affected component/version,
reproduction, impact and whether any public development environment was tested.
Do not access other users' data or degrade a shared service while validating a
report.

## Production gaps

Production readiness still requires, at minimum, approved cryptography and key
lifecycle, multi-validator finality with authenticated set transitions,
independent deployment review, abuse/fraud controls, observability with secret
redaction, backups and restore exercises, dependency/SBOM attestations, incident
response and legal/licensing approval.

