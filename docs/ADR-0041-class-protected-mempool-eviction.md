# ADR-0041: Class-protected bounded mempool eviction

## Status

Accepted for the Rust reference implementation.

## Context

ADR-0040 gives retail and system traffic a bounded share of a proposal once an
operation is resident in the local pool. It does not help when valid standard
traffic has already consumed the global operation or byte capacity: a new
checkout can still be rejected before candidate selection.

Unrestricted priority eviction would create a stronger denial-of-service tool.
A peer must not be able to label its transaction as retail or system traffic,
equal-priority traffic must not churn existing work, one admission must not
cause unbounded scanning or removal, and a failed replacement must not corrupt
global, sender or pending-request accounting.

## Decision

Peer gossip and generic local ingress remain `Standard` and use strict bounded
admission. They never evict a resident operation. A trusted local
`publish_local_with_class` call may plan replacement only when global operation
or byte capacity is exhausted, and only entries with a strictly lower
`TrafficClass` are eligible:

- `Retail` may evict `Standard`;
- `System` may evict `Standard` and then `Retail`;
- no class may evict the same or a higher class.

Eligible victims are ordered by lowest class first, largest envelope first,
newest process-local admission sequence next and operation ID last. This frees
the required bytes with fewer removals while protecting older accepted work
when class and size are equal. One admission may remove no more than the
configured non-zero eviction bound. If the bounded eligible set cannot make
room, the new operation fails without mutation.

An entry from the same sender with a nonce less than or equal to the incoming
nonce is never eligible. Removing it could strand the newly admitted operation
behind a missing consensus nonce or silently introduce replacement-by-fee
semantics. A higher nonce from the same sender may be displaced by an earlier
incoming nonce because that preserves forward execution order.

The pool computes the complete replacement before committing it: operation and
byte totals, per-sender operation and byte usage, pending-request cleanup and
the next admission sequence must all validate. Only then are victims removed,
the new entry inserted and every secondary counter published. Sender quotas
remain authoritative; a higher traffic class cannot use eviction to exceed its
sender allowance.

`LocalPublication` returns the admitted operation ID, whether it was newly
inserted and the exact bounded set of locally evicted operation IDs. This is an
adapter/telemetry input, not a consensus receipt. A byte-identical known
operation still only promotes its class and reports no insertion or eviction.

## Security and operational boundaries

- Traffic class is trusted process-local metadata. Network frames contain no
  class, and peer-delivered operations always enter as `Standard`.
- The current payment-gateway adapter is the only production path assigning
  `Retail`. No external `System` endpoint exists.
- Eviction does not reject or reverse a ledger operation. Another node may
  retain it, and a client may safely retry or re-gossip the exact envelope.
- Admission sequence and victim choice are local and are not included in signed
  bytes, block payloads or consensus state.
- ADR-0043 adds exact low-cardinality eviction/activity counters and validated
  pool-pressure snapshots. Durable pool recovery, cross-node classification,
  reputation, age-aware promotion and shared policy distribution remain
  production work.
- Candidate fair shares, signature verification, nonce ordering, fees, expiry
  and runtime monetary checks remain unchanged and authoritative.

## Consequences

A saturated standard pool no longer blocks an authenticated retail checkout,
and a future trusted system operation can displace lower-class work. Untrusted
peer traffic cannot trigger displacement, equal-class floods cannot churn the
pool, the number of removals is explicitly bounded, and rejected replacements
leave primary and secondary state unchanged.

Tests cover lower-class-only eviction, standard-before-retail victim choice,
newest-entry choice, eviction reporting, sender-accounting release, nonce
predecessor preservation, bounded planning and failure atomicity when a
replacement would violate sender quota.
