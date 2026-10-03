# ADR-0027: Signed bootstrap network configuration

## Status

Accepted for the reference bootstrap, authenticated transport, snapshot and
LMDB boundaries. Dynamic governance updates, key rotation and threshold/HSM
authorization remain separate production gates.

## Context

ADR-0026 introduced a height-gated state-commitment migration. Passing its
activation height as an ordinary command-line or constructor value would still
allow honest validators to run different consensus rules. A node also needs one
identity that binds its expected network, genesis checkpoint, initial validator
set and protocol compatibility before it accepts peer or snapshot data.

A configuration that carries its own signing key is not self-authenticating.
The node must compare that key with a separately provisioned trust anchor before
accepting the signature.

## Decision

- `network-config-core` defines a fixed-size canonical `NetworkConfig` containing
  the network ID, complete genesis checkpoint, non-zero protocol version,
  configuration public key and state-commitment activation policy.
- The configuration rejects zero trust/checkpoint identifiers and an activation
  height before genesis. Authenticated state can begin at genesis by setting the
  activation height equal to the genesis height.
- `NetworkConfigCodec` is a canonical 197-byte encoding. The signed envelope is
  a canonical 277-byte encoding. Unknown domains, lengths, policy tags and
  non-canonical absent-policy bytes fail closed.
- A domain-separated SHA-256 digest of the canonical configuration is the
  `ProtocolDigest` already carried by peer admission, channel-bound sessions and
  snapshot manifests.
- The configuration signature covers that digest under a separate authorization
  domain. Verification requires the expected network and an externally pinned
  configuration public key; trusting the key contained in unverified bytes is
  forbidden.
- `network-config-auth-ed25519` provides the strict reference Ed25519 verifier
  behind the core verification trait. This does not decide the final governance
  key scheme.
- A `VerifiedNetworkConfig` capability is created only after all identity and
  signature checks pass. It exposes the protocol digest, genesis checkpoint and
  commitment policy to downstream components.
- Ledger-mode LMDB can be opened only with that verified capability. Genesis
  initialization must exactly match the signed checkpoint and state root, then
  persists the protocol digest and commitment policy in the same transaction as
  state, JMT nodes and cursors.
- Reopening with a missing or different config fails closed. The generic
  runtime-neutral byte-state store remains available through a separate open
  path and cannot initialize or inspect normalized ledger state.

## Evidence

Golden-vector tests pin the protocol digest and round-trip both canonical
codecs. Negative tests cover wrong network, wrong trust anchor, invalid
signature, non-canonical policy encoding, zero fields and activation before
genesis. The Ed25519 adapter verifies a real signature and rejects a bit-flipped
one.

The LMDB integration test initializes through a verified configuration, crosses
the authenticated-root activation height, rejects both unsigned reopen and a
differently signed policy, then continues after a correct restart. The process
recovery harness derives its peer/snapshot `ProtocolDigest` from the same signed
configuration and finishes signed payment catch-up through height 103.

## Consequences

The root-migration schedule is no longer an informal operator setting in the
reference ledger path. A peer with a different signed bootstrap config has a
different protocol digest and cannot establish a compatible admitted session;
an existing LMDB environment also refuses it.

The current reference uses one pinned Ed25519 configuration key. Production
requires an offline/HSM or threshold-controlled trust root, documented recovery
and rotation, signed config distribution, rollback protection for config
versions, and governed post-genesis update rules. The signed bootstrap config
contains the validator-set hash, not the full authority set; that set remains
validated by the finality/authority-history boundary.
