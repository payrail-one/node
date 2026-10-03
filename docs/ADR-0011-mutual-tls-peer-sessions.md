# ADR-0011: Mutually authenticated and channel-bound peer sessions

## Status

Accepted for the first socket harness. QUIC remains a measured transport
candidate, not an approved production dependency.

## Context

Application-level Ed25519 membership proves possession of a registered node
key, but it does not encrypt traffic or by itself bind every later frame to the
same connection. TLS authentication alone is also insufficient: a certificate
issued by the consortium CA must not grant arbitrary validator identity or
role. Both layers must agree and reconnects must not make old frames reusable.

## Decision

1. The reference socket adapter uses pinned `rustls 0.23.45`, TLS 1.3, mandatory
   client certificates and the neutral `payment-node` ALPN. TLS 1.2 and early
   data are disabled by crate features/configuration.
2. Every membership record contains a unique domain-separated SHA-256
   fingerprint of its expected leaf certificate. Admission returns that
   fingerprint together with network, epoch, protocol and fresh challenge.
3. After TLS and membership authentication, the session compares the observed
   peer leaf certificate with Admission. A CA-valid but differently registered
   certificate fails closed.
4. Both peers export 32 bytes of TLS keying material with a dedicated exporter
   label. Directional session identifiers bind this channel value to network,
   node, role, membership epoch, protocol, fresh challenge, transport key and
   certificate fingerprint.
5. Every application frame carries the directional session identifier and an
   exact monotonic sequence. Replays, gaps and frames copied from another TLS
   connection are rejected before gossip decoding.
6. Wire-frame length is checked before allocation. The outer TLS frame cannot
   exceed the already bounded session/gossip payload.
7. Private keys are supplied in memory through `TlsIdentity`; the adapter does
   not read keys from repository paths, environment variables or command-line
   arguments. Production loading belongs behind an HSM/secret-store boundary.

Rustls is a low-level encrypted-pipe library and performs certificate
verification from configured roots; its documentation states a Rust 1.71 MSRV,
which is below this workspace's Rust 1.85 baseline:
[rustls documentation](https://docs.rs/rustls/0.23.45/rustls/).

## Current evidence and limits

- Integration tests establish real loopback TCP connections with mutual TLS,
  verify ALPN, compare the TLS exporter on both ends and exchange bounded
  frames.
- A seven-node loopback star test verifies six distinct client certificates at
  the seventh node. It is a transport harness, not a consensus/finality result.
- Certificate expiry, rotation overlap, CRLs/OCSP policy, HSM keys, process
  isolation and production PKI ceremonies remain required.
- The current harness is synchronous and does not establish throughput or
  Internet latency.

## QUIC gate

Quinn exposes TLS 1.3 encrypted QUIC connections, independent streams,
congestion control and avoids cross-stream head-of-line blocking:
[Quinn documentation](https://docs.rs/quinn/0.11.12/quinn/). It remains the
preferred benchmark challenger for transaction, block and snapshot streams.
It is selected only after the seven-validator loss/latency benchmark compares
it with the simpler TLS/TCP reference and threat-model review approves UDP
operations, certificate binding, rate limits and observability.
