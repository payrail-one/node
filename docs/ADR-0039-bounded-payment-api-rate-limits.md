# ADR-0039: Bounded payment API rate limits with idempotent retry classification

## Status

Accepted for the Rust reference implementation.

## Context

Peer and signed-sender mempool quotas bound resident transaction memory, but
they act after an API request has reached payment orchestration. An authenticated
client can still generate excessive journal lookups, policy calls, nonce reads,
signing work and publication attempts. Rotating client request keys bypasses
ordinary idempotency while rotating sender accounts can bypass a single-account
limit.

Applying every quota again to an exact recovery retry is also unsafe. A wallet
or merchant recovering an indeterminate publication must reuse the exact
journaled request; repeatedly charging tenant and account intent quotas could
prevent that recovery and leave strict nonce sequencing stuck.

## Decision

`payment-ingress-core` is a transport-independent boundary in front of the
durable `PaymentGateway`. It validates the canonical request, network and
authorized identity, applies principal preflight, then reads the authoritative
idempotency journal and classifies the call before invoking the gateway:

- a new request ID is classified as a new intent;
- an existing request ID is classified as an existing request, whether its
  digest is an exact retry or a conflict.

Every structurally valid, network-bound and identity-matched request first
consumes API-principal quota before the journal lookup. A denied principal
therefore cannot create unbounded random-key reads. If the journal confirms a
new request ID, tenant and account buckets are prepared together before either
token is committed. A tenant/account denial does not partially charge the other
monetary-identity scope, although the principal attempt remains charged for the
work already admitted.

An existing request consumes no further tenant/account quota. An exact digest
continues through the gateway's existing exact-envelope recovery path. A
conflicting digest returns `RequestConflict` after the principal charge and
never reaches the gateway. This preserves safe retry without permitting a known
request ID to become an unlimited lookup endpoint.

The reference limiter uses integer token buckets with one token restored per
configured interval. It rejects zero/overflowing configuration, monotonic-clock
regression and timestamp overflow. Identity maps and their expiration indexes
are strictly bounded. An idle entry may be evicted only after at least its full
bucket recovery duration, so eviction cannot restore capacity earlier than
ordinary refill.

The ingress identity contains a fixed opaque API principal plus its authorized
tenant and account scope. It must be derived from authenticated and authorized
transport credentials by the future HTTP/gRPC/SMS adapter and is never accepted
from an untrusted request body. The service requires its tenant and account to
match the validated canonical payment request before journal lookup or quota
mutation.

## Security and operational boundaries

- The limiter is local admission policy, not ledger consensus or monetary
  authorization. Validators still verify every signed block operation.
- The in-memory reference state is process-local and intentionally has no
  hidden I/O. ADR-0042 adds a durable atomic LMDB adapter for multiple processes
  on one host. Multi-host APIs still require an authoritative transactional
  backend or sticky, failure-aware sharding with fail-closed degradation.
- The host must authenticate the principal, authorize the tenant/account scope,
  serialize access to a limiter instance and supply a trusted monotonic
  timestamp. Client-provided identities and timestamps are forbidden.
- Parsing limits, unauthenticated traffic limits, connection limits and network
  DDoS protection remain transport/edge responsibilities.
- A race between journal classification and gateway reservation can
  conservatively charge two simultaneous first attempts. The idempotency store
  remains authoritative and prevents duplicate nonce allocation or signing.
- Signed configuration distribution, policy migration, aggregate telemetry,
  reputation, dynamic congestion pricing and multi-host/cross-region quota
  coordination remain open production work. ADR-0040 separately provides local
  fair candidate classes after ingress.

## Consequences

One API credential cannot create unbounded journal lookups, and one tenant or
sender account cannot create unbounded new payment work in a single reference
service process. Exact durable retries remain possible without repeatedly
consuming tenant/account intent capacity, while conflicting retries are still
bounded by authenticated principal identity.

Tests cover principal preflight before journal reads, atomic tenant/account
denial, token refill, safe idle eviction, bounded identity capacity, clock
regression, overflow, identity/network mismatch, account-limit denial before
journal mutation, exact gateway retry and conflict handling.
