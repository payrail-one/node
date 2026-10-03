# ADR-0037: Finalized-height transaction admission and expired cleanup quota

## Status

Accepted for the Rust reference implementation.

## Context

Consensus expiry prevents a delayed signed payment from moving value and avoids
an account nonce gap, but expiry alone is not a safe public mempool policy. An
unbounded future validity height retains work indefinitely. A very old expired
operation has no reason to enter normal gossip because the account can sign a
replacement at the same nonce. Expired execution also charges no fee, so it
must not consume an unlimited share of a payment block.

Admission cannot use a height captured when a long-lived validator object was
constructed. That height would become stale while the node keeps running. A
single minimum fee is also invalid for a multi-asset ledger because assets have
different atomic units and economic values.

## Decision

Every local or peer transaction admission receives an explicit
`AdmissionContext` containing the latest locally verified finalized height.
The context is supplied per call. The LMDB-backed local payment publisher reads
the atomically stored finalized cursor before every publication.

`AdmissionPolicy` applies before signature verification and contains:

- one explicit minimum atomic fee for every admitted `AssetId`;
- a maximum validity horizon measured from the finalized height;
- a bounded window in which a recently expired operation may enter the nonce
  cleanup path.

An empty or duplicate fee schedule and zero height windows are invalid policy.
An unlisted asset, insufficient fee, excessive future horizon, stale cleanup
request or finalized-height overflow receives a distinct fail-closed rejection.
A zero fee is possible only when explicitly configured for that asset, such as
on an isolated test or regulated private deployment.

The block proposal has a separate `max_expired_operations` limit. An expired
operation counts against both that quota and the ordinary block operation/byte
limits. Once the cleanup quota is exhausted, further expired candidates are
deferred while independent valid payments can still be selected. For blocks
larger than one operation, configuration requires the cleanup quota to be
strictly smaller than total operation capacity.

Already stored operations preserve exact retry semantics as height advances.
Only byte-identical envelopes are `Duplicate`/`AlreadyKnown`; an envelope with
the same operation ID but different authorization bytes is rejected as a
conflict and never replaces the accepted bytes.

## Security and operational boundaries

- The height source is a trusted boundary. API clients and peers never select
  the finalized height used by policy.
- The fee schedule and height windows are deployment configuration and require
  signed governance/configuration distribution before production.
- Changing admission policy requires coordinated activation and pool
  flush/re-admission; hot policy migration is not implemented by this reference.
- The minimum signed fee does not make expired execution economically costly,
  because expiry intentionally charges no fee. The proposal cleanup quota,
  bounded pool and short cleanup window contain that work.
- Peer-owned pending-request quotas and per-sender resident operation/byte
  quotas are provided by ADR-0038. ADR-0039 adds process-local time-window
  account/API-principal/tenant limits before gateway work. Shared multi-replica
  enforcement, reputation, safe mempool eviction and dynamic congestion pricing
  remain required before unrestricted public ingress.
- Validators still re-verify and re-execute block payloads. Pool admission is
  not consensus finality and cannot authorize monetary state by itself.

## Consequences

The reference ingress now rejects far-future retention, ancient cleanup spam,
unsupported fee assets and below-floor transactions before expensive signature
verification. Tests cover exact policy boundaries, height overflow, unsupported
assets, forged signatures, height refresh from the finalized source, exact
retry after the window moves, conflicting envelope bytes and the independent
expired-operation proposal quota.
