# ADR-0043: Low-cardinality mempool observability

## Status

Accepted for the Rust reference implementation.

## Context

ADR-0040 and ADR-0041 protect proposal shares and admission capacity for retail
payments, but an operator still needs to see pool pressure, abuse, priority
eviction and cleanup. Exporting account or operation identifiers as metric
labels would create unbounded cardinality and leak payment metadata. Letting an
unavailable metrics backend participate in admission would also turn
observability into a payment outage.

The pool already maintains primary envelopes and secondary byte, sender and
pending-peer indexes. A metrics view must not introduce another authoritative
accounting implementation or influence transaction validity, ordering or
finality.

## Decision

`transaction-gossip-core` exposes an exact `MempoolSnapshot` with:

- current operations and bytes split by `Standard`, `Retail` and `System`;
- configured operation, byte and pending-request capacity;
- pending-request, pending-peer and tracked-sender gauges;
- process-lifetime admitted, duplicate and rejected decisions by bounded ingress
  origin;
- admitted, promoted and evicted operation/byte counters by traffic class;
- resolved operation/byte counters;
- stable low-cardinality rejection kinds and a separate internal-failure count.

The current gauges are recomputed from primary entries when requested. Before a
snapshot is returned, their byte and sender totals must exactly equal the
secondary indexes, and the complete peer-count index must equal a reconstruction
from the primary pending map. Any disagreement returns
`PoolAccountingInconsistent`; corrupt state is never published as healthy
telemetry.

Activity counters update only after the corresponding primary mutation commits,
except rejection and internal-failure counters, which describe attempts that did
not mutate the pool. A byte-identical retry is counted as a duplicate; a trusted
class promotion is counted separately. Batch reports count every stable per-item
decision, while a top-level validator or accounting failure is one internal
failure for that batch call.

Counters use saturating arithmetic and expose `counter_saturated`. Exhausted
telemetry capacity must never reject an otherwise valid payment. Current pool
accounting continues to use checked arithmetic and remains authoritative.

The snapshot contains no account, peer or operation identifiers. Exact evicted
operation IDs remain only in the bounded `LocalPublication` result so the local
adapter can retry or diagnose a specific replacement without turning it into a
metric label.

## Security and operational boundaries

- Metrics are local operational evidence, not consensus state or a receipt.
- Counters reset when the process restarts. A metrics backend may aggregate
  process epochs; it must not restore counters into pool state.
- Export is pull-based from the snapshot boundary. Prometheus/OpenTelemetry or
  another vendor adapter belongs in the service host and cannot block admission.
- Traffic classes and rejection kinds are a fixed bounded label set. Account,
  peer and operation identifiers must be logs/traces with access controls, not
  metric labels.
- Pool capacity and activity can drive alerts and dashboards. They do not yet
  implement dynamic congestion pricing, reputation or age-aware promotion.
- Durable mempool recovery remains separate production work.

## Consequences

Operators can measure class pressure, client-visible rejection causes, internal
failures, priority displacement and finality cleanup without duplicating the
pool's monetary or sender accounting. Snapshot validation also gives tests and
health checks a fail-closed consistency audit. Telemetry overflow and exporter
failure cannot stop the payment path.

Tests cover local, batch and peer admission origins, malformed and wrong-network
rejections, class promotion, exact eviction bytes, resolved bytes, internal
failure separation, saturation and corrupt-index detection.
