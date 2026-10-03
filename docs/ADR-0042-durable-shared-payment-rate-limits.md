# ADR-0042: Durable shared payment rate limits

## Status

Accepted for the Rust reference implementation and same-host API processes.

## Context

ADR-0039 bounds authenticated payment ingress with process-local principal,
tenant and account token buckets. Running several API workers with independent
buckets multiplies every configured allowance and allows a caller to bypass the
intended limit by reaching another worker. Process restart also restores local
capacity early.

A shared limiter must preserve the existing gateway semantics: every valid API
attempt consumes principal quota before journal lookup, while tenant and account
quota for a journal-confirmed new intent must be charged together or not at all.
An exhausted or corrupt scope must not partially charge another scope.

## Decision

`payment-ingress-core` remains storage-independent through
`PaymentIngressRateLimiter`. Its validated `PaymentRateLimitConfig` exposes
read-only policy accessors so an adapter can bind durable state to the exact
configuration.

`payment-rate-limit-lmdb` provides a local-disk shared adapter for multiple API
processes on one host:

- one LMDB environment contains separate principal, tenant and account bucket
  tables plus a canonical expiration index;
- normal LMDB locking and its single-writer transaction serialize competing
  processes;
- a principal decision is one write transaction;
- tenant and account preparation, validation and token consumption are one
  write transaction;
- network ID and the exact canonical bucket, tracking and idle policy are
  immutable metadata bindings; a mismatched process fails before admission;
- the last accepted service timestamp forms a durable logical high-water mark;
  a lower concurrent timestamp is clamped to that mark, so interleaved request
  phases do not fail and time can never restore capacity early;
- tracking capacity may reuse only an indexed entry whose idle deadline has
  passed. The policy constructor already requires that deadline to be at least
  the complete token recovery window;
- opening the store validates bucket sizes, token bounds, expiration ownership,
  table cardinalities, network and configuration before serving traffic;
- storage, time, integer and accounting failures abort the LMDB transaction.

The caller supplies a trusted, globally comparable millisecond timestamp. For
this persistent adapter it must not be a process-relative clock that resets on
restart. A production host should use a secured time source and alert when the
input falls behind the durable logical clock. A forward jump can refill buckets
and therefore remains a protected operational event.

## Unsafe boundary

`heed::EnvOpenOptions::open` uses memory mapping and is therefore unsafe. One
localized unsafe block is allowed under the same constraints as other reviewed
LMDB adapters:

1. the canonical directory is application-owned, local and not a symlink;
2. live LMDB files are never modified outside LMDB;
3. locking and full durability remain enabled;
4. mapped references never escape transaction lifetimes;
5. no `NO_LOCK`, `WRITEMAP`, `NO_SYNC` or network-filesystem deployment is used.

## Security and deployment boundaries

- This adapter shares limits across processes opening one environment on one
  machine. It is not replicated storage and must never be placed on NFS or a
  distributed filesystem.
- Multi-host and multi-region API replicas require another implementation of
  the same port backed by an authoritative transactional service. It must use
  backend-side atomic time and decisions; client-side read/modify/write is not
  acceptable.
- A policy change currently requires an explicit migration or a new empty
  environment. Silently opening existing buckets with different limits is
  forbidden.
- Edge connection limits, unauthenticated DDoS filtering and transport parsing
  bounds remain outside this payment-identity limiter.
- Exact journal retries still skip tenant/account charging but continue to
  consume shared principal quota through the unchanged ingress service.

## Consequences

Restart no longer restores quota, multiple local API workers cannot each spend
the same allowance, and simultaneous writers cannot both consume the final
token. Tenant/account denial remains failure-atomic across processes. The
adapter adds local write serialization and disk I/O, which must be included in
checkout latency and capacity tests before deployment.

Tests open the same environment from independent OS processes, exercise
simultaneous final-token contention, verify cross-process tenant/account
atomicity, interleaved timestamp clamping, restart persistence, safe expiration
reuse, timestamp overflow handling, and network/configuration binding.
