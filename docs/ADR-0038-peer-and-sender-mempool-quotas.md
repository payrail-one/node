# ADR-0038: Peer-owned pending requests and sender-isolated mempool quotas

## Status

Accepted for the Rust reference implementation.

## Context

Global operation and byte limits bound total memory, but do not provide fair
admission. One authenticated peer could consume every outstanding request by
announcing operation identifiers without returning payloads. One signed account
could also occupy the resident pool and prevent unrelated retail payments from
entering it. Permissioned membership reduces exposure but does not remove this
local denial-of-service risk.

Pending requests and resident transactions have secondary accounting indexes.
If those indexes diverge from their primary records, best-effort cleanup could
partially mutate the pool and silently corrupt later capacity decisions.

## Decision

Every requested operation identifier is bound to the authenticated `NodeId`
that announced it. The pool enforces both a global pending-request limit and a
per-peer limit. A transaction payload is accepted only from the owning peer. A
payload from another peer neither satisfies nor releases the request.

An owning peer's response is terminal for that request: the pending slot is
released before canonical decoding, identifier matching and transaction
validation. A malformed, mismatched or rejected payload therefore cannot pin a
slot indefinitely. The node-service adapter must also call the explicit
single-request timeout cancellation or whole-peer disconnect cancellation path
when no response arrives.

Resident transactions are charged to the sender carried by the completely
decoded signed operation. The pool enforces per-sender operation and envelope
byte limits in addition to the existing global limits. Exact known-envelope
retries remain idempotent and consume no additional quota; conflicting bytes for
the same operation identifier remain rejected.

Single insertion validates global and sender capacity before mutation. Batch
admission aggregates every candidate for each sender before mutation. When one
sender's batch group exceeds its operation or byte quota, the whole new group
for that sender receives a stable rejection while independent senders may still
enter. The remaining accepted set is then subject to the existing aggregate
all-or-none global capacity decision.

Finality/runtime resolution removes primary entries and updates global bytes,
per-sender usage and any matching pending ownership indexes together. Cleanup
first validates all affected secondary accounting; inconsistency returns a
fail-closed error without partial mutation. Replayed resolution notifications
remain idempotent.

## Security and operational boundaries

- These quotas are local admission policy, not consensus rules or finality.
- Peer ownership is trustworthy only when the transport supplies the `NodeId`
  established by membership validation and mutual authentication.
- Per-sender quotas limit one signed account, but an attacker may create or
  compromise multiple accounts. ADR-0039 adds process-local time-window
  account, API-principal and tenant limits at payment ingress; shared
  multi-replica enforcement remains required for production.
- The future node service owns timeout and disconnect lifecycle calls. This
  domain crate does not run clocks or transport sessions.
- ADR-0040 adds local fair traffic classes and ADR-0041 adds bounded
  lower-class-only eviction. Reputation, dynamic congestion pricing, durable
  mempool recovery and multi-node admission coordination remain open.
- Limits are deployment configuration and require capacity testing and signed
  configuration distribution before public operation.

## Consequences

An admitted peer can no longer monopolize every pending-request slot within its
own configured quota, and one signed sender cannot fill the entire resident
pool. Invalid terminal responses and disconnects release ownership
deterministically. Batch admission isolates an abusive sender without discarding
valid work from unrelated senders, while aggregate capacity still fails before
any insertion.

Tests cover peer ownership, wrong-owner responses, terminal invalid responses,
timeouts, disconnect cleanup, per-peer capacity, sender operation and byte
capacity, batch sender isolation, exact quota release after resolution and
failure atomicity under deliberately corrupted secondary indexes.
