# ADR-0008: Permissioned consensus membership and safe state sync

Status: architecture constraint; implementation follows the node-stack bake-off

## Context

The payment network needs rapid propagation and node catch-up without allowing
an unknown machine to join consensus, impersonate a validator or feed a false
state snapshot. Encryption alone proves only possession of a transport key; it
does not grant a consensus role. IP allowlists and temporary bans are useful
defence in depth but are not cryptographic membership.

## Decision

1. The consensus plane is permissioned initially. Public wallets and merchants
   connect to API/sentry nodes, never directly to validator RPC or signing
   interfaces.
2. Every node handshake binds the full `NetworkId`, stable node ID, current
   membership epoch, requested role, protocol capabilities and a fresh server
   challenge. The node proves possession of its registered transport key.
3. Transport identity and consensus signing identity are distinct. Validator
   keys live behind a remote signer/HSM; compromise of a P2P certificate must
   not enable block or finality signatures.
4. Validator and trusted-sync membership is rooted in genesis and changed only
   by finalized multi-party governance. Changes have an activation height,
   monotonic epoch and explicit revocation; stale epochs are revalidated.
5. Unknown nodes, wrong-network nodes, revoked nodes, premature activations,
   duplicate keys and roles absent from the membership record fail closed.
6. Validators sit behind sentry nodes. Consensus RPC is not internet-exposed;
   public API, indexing and explorer workloads run on separate nodes.
7. Fast sync may download bounded chunks concurrently from several admitted
   providers, but trust comes from verification rather than the provider:
   snapshot manifest, chunk hashes/state root, finalized block hash and the
   consensus finality proof must all agree with the local genesis/checkpoint.
8. Membership records bind a unique TLS certificate fingerprint in addition to
   the application transport key. A valid consortium certificate alone does
   not authorize a node identity or role.
9. A node cannot vote or serve authoritative data until snapshot verification,
   state application and tail catch-up complete.
10. Peer quotas, connection limits, message bounds, reputation and rate limits
   protect availability, but none can override cryptographic authorization.

The reference implementation places policy in `network-membership-core` and
strict Ed25519 proof verification in the separate `network-auth-ed25519`
adapter. This keeps membership rules independent of the final transport and
validator cryptography while providing a real fail-closed authentication path.
The adapter pins `ed25519-dalek` to the same exact dependency used for payment
authorization, disables default features and uses strict verification. A node
server must generate challenges with an operating-system CSPRNG, scope them to
one connection attempt and never accept a challenge twice.

## Key lifecycle

- Node enrolment records owner, roles, transport key, optional validator key,
  activation height and operational metadata hash.
- Rotation is an authenticated membership change with an overlap window; old
  keys expire at a finalized height.
- Emergency revocation propagates through sentries and validators and prevents
  new sessions immediately after the governed activation point.
- Consensus double-sign protection persists across restart and validator
  replacement. Remote signing policy binds network, validator, height and round.
- Certificates, membership records and signer keys have separate backup and
  recovery procedures.

## Legacy disposition

The old system's TLS, peer database, height comparison and retry logic are
useful evidence of operational requirements. The bcrypt-protected shared peer
secret, rotating application token, Redis attempt counter, central master and
random block comparison are not sufficient membership or finality controls and
are not reused as the new trust boundary.

## Security consequences

- A machine with network reachability but no active membership proof cannot
  enter the consensus mesh or become a trusted sync source.
- A compromised sync provider cannot forge state if the receiving node verifies
  the finalized state root and every chunk.
- Governance compromise remains a critical risk; membership changes therefore
  require threshold approval, audit events, delayed activation and emergency
  procedures.
- Permissioned membership does not make application traffic private. Sensitive
  customer data remains off-chain and transport encryption is still mandatory.

## Rejected alternatives

- IP allowlist only: addresses change, can be routed incorrectly and do not
  prove possession of validator keys.
- One shared network password: compromise of one node compromises admission for
  every node and makes individual revocation unreliable.
- Trusting the fastest or highest peer: an attacker can advertise false height
  or state.
- Shipping validator private keys inside node images or Compose files: expands
  key exposure and prevents defensible ceremonies and rotation.
