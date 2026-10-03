# ADR-0016: local multi-process validator harness

## Status

Accepted as a local integration harness. It is not GRANDPA, a validator peer
mesh, a production benchmark, or evidence of remote failure-domain isolation.

## Context

The deterministic consensus lab validates quorum and fault-model logic but does
not exercise process isolation, TCP, TLS handshakes, certificate identities or
inter-process signing. The next evidence step needs seven actual OS processes
without prematurely coupling the domain code to a node framework.

## Decision

- `tools/validator-process-harness` starts exactly seven validator child
  processes on loopback TCP addresses.
- Every process receives a unique TLS server certificate and consensus signing
  key. The coordinator has a distinct client certificate.
- Connections require TLS 1.3 mutual authentication and the coordinator and
  validator both pin the expected peer certificate fingerprint.
- A validator checks network ID, membership epoch and validator-set hash before
  signing the canonical finality message with Ed25519.
- The coordinator collects responses concurrently, constructs a canonical
  weighted finality certificate and verifies it with the shared strict verifier.
- Fault scenarios start all seven processes but deliberately leave selected
  validators without a request, modeling non-response at this integration layer.
- TLS and signing keys exist only in a unique temporary directory. Private files
  use owner-only permissions on Unix, are never printed and are removed when the
  harness exits. Embedded consensus seeds remain reproducible lab fixtures only.

## Limits

The coordinator is an external test controller, not a consensus participant.
Validators do not yet form a peer-to-peer mesh, execute GRANDPA rounds, persist
blocks, apply state sync, or run on independent hosts. The reported timing is
actual local mTLS vote-quorum collection time, not end-to-end finality, payment
latency, TPS, or a geographic-network result. A later harness must run real node
services under network fault injection locally and across separate hosts.
