# ADR-0010: Bounded transaction gossip and durable snapshot chunks

## Status

Accepted for the prototype. This is not the final transport or consensus
selection.

## Context

Fast payment propagation must not allow an authenticated or compromised peer to
force unbounded allocation, inject another network's transaction, poison an
operation identifier with invalid signatures, or claim snapshot progress before
bytes are durable. Permissioned membership reduces exposure but is not a DoS or
data-integrity control by itself.

## Decision

1. Transaction propagation uses `announce -> request -> transaction`, rather
   than broadcasting every full payload to every peer.
2. Every frame binds the network and current membership epoch. The transport
   passes the `Admission` created by the authenticated peer handshake; validator
   and sentry roles may relay transactions, while a sync-only role may not.
3. Frames and transaction envelopes have hard byte bounds and reject unknown
   variants, truncation and trailing bytes.
4. A node accepts a transaction payload only after requesting that operation
   identifier. It canonically decodes the envelope, checks its network and
   recomputes the operation identifier before invoking a replaceable signature
   and policy validator.
5. Pending identifiers, operation count and aggregate envelope bytes are
   independently bounded. Capacity failures do not mutate accepted state.
6. The propagation pool does not decide consensus ordering or finality. A
   transaction seen in gossip is pending, not paid or finalized.
7. Verified state-sync chunks cross a storage abstraction before session
   progress advances. The filesystem adapter rechecks length and hash, writes a
   new temporary file, calls `sync_all`, atomically publishes with a hard link,
   and syncs the containing directory. Existing content is revalidated.
8. Store paths are derived only from fixed-size hashes and chunk indexes. The
   root and manifest directory are checked against symbolic links. Production
   deployment must additionally enforce exclusive OS ownership and restrictive
   permissions; portable Rust path checks cannot eliminate every local
   privileged race.

## Consequences

- Bandwidth amplification and memory use are bounded before a production P2P
  framework is selected.
- A transport adapter can use QUIC, libp2p or another reviewed implementation
  without moving peer authorization or pool invariants into framework code.
- Invalid, unsolicited and cross-network payloads fail before pool mutation.
- Disk failure cannot advance sync progress, and crash retries do not duplicate
  content.
- The pool now has finalized-height fee/expiry admission and peer/sender
  isolation through ADR-0037 and ADR-0038. It remains an intentionally bounded
  reference mempool: ADR-0041 provides bounded lower-class-only local eviction,
  and ADR-0043 adds low-cardinality activity/pressure observability with
  fail-closed accounting validation. Reputation and persistent recovery remain
  required. ADR-0039 separately bounds process-local payment API ingress, and
  ADR-0040 adds local fair traffic classes.
- The filesystem adapter stores chunks but does not yet reconstruct session
  progress or atomically activate an applied state database.

## Required next validation

- Connect the protocol to mutually authenticated encrypted transport and prove
  that the transport cannot substitute an `Admission` between connections.
- Run seven validators across three failure domains with delay, loss,
  duplication, partitions and membership revocation during propagation.
- ADR-0042 shares API quotas across same-host processes. Add a multi-host atomic
  quota adapter and mempool reputation before accepting unrestricted
  Internet-facing traffic; ADR-0043 supplies the bounded eviction and pressure
  metric source, but a non-blocking service exporter is still required.
- Implement restart recovery from stored chunks and atomic state-database
  activation after the finalized state root is reproduced.
