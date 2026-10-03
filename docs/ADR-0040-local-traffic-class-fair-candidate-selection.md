# ADR-0040: Local traffic classes and fair candidate selection

## Status

Accepted for the Rust reference implementation.

## Context

Selecting only the lowest operation identifiers from a saturated pool gives no
latency protection to retail checkout or critical internal operations. A flood
of otherwise valid background transactions can fill the bounded candidate
window even though per-peer, per-sender and API rate limits are working.

Naive priority is also unsafe. A peer or user must not be able to label its own
transaction as critical, and a high-priority class must not starve ordinary
traffic indefinitely. Runtime proposal building requires the input candidate
set to remain strictly ordered by operation ID, so class ordering cannot be
passed directly into deterministic execution.

## Decision

The local pool records one non-consensus `TrafficClass` with each accepted
envelope:

- `System` for explicitly trusted internal control/settlement adapters;
- `Retail` for the authenticated payment-gateway publication path;
- `Standard` for peer gossip, direct generic ingress and background work.

The gossip protocol contains no traffic-class field. Every peer-delivered
transaction is therefore `Standard`, regardless of peer role. A byte-identical
trusted local retry may promote an existing entry but never downgrade it. A
peer response cannot alter existing classification.

`CandidateSelectionPolicy` defines a bounded window and one saturated
allocation per class. For windows of at least three, construction requires a
non-zero allocation for every class and the allocations must sum exactly to the
window. The default payment policy assigns approximately one eighth to system,
three quarters to retail and one eighth to standard traffic. Very small windows
cannot guarantee all three classes and are handled explicitly.

Selection first fills each class allocation in operation-ID order. Unused
capacity is then borrowed in `System`, `Retail`, `Standard` order. Thus every
populated class keeps its saturated allocation, while an empty class never
wastes candidate capacity. The final selected subset is re-sorted strictly by
operation ID before entering `ledger-runtime-core`; the existing deterministic
sender/nonce scheduler, signature checks, fees, expiry and monetary rules remain
authoritative.

The class is local proposer policy and is not included in signed transaction
bytes, operation identity, block payload or consensus state. Different
proposers may select different valid subsets, as they already can from different
local pools. Every finalized block is still independently verified and executed.

## Security and operational boundaries

- Transport adapters must not construct `System` from an untrusted request.
  No system-class HTTP/SMS endpoint exists in the reference implementation.
- `Retail` indicates trusted gateway routing, not stronger monetary authority;
  it cannot bypass signatures, nonce order, balance checks or policy.
- A higher-class transaction cannot skip a lower-nonce predecessor from the
  same account. It remains deferred until consensus nonce dependencies resolve.
- Pool classification is process-local and disappears with the in-memory pool.
  Durable mempool recovery and cross-node classification coordination remain
  open.
- ADR-0041 adds bounded lower-class-only eviction for trusted local admission,
  and ADR-0043 adds validated low-cardinality operational metrics. Reputation,
  age-aware promotion, dynamic congestion pricing, signed policy distribution
  and a non-blocking service exporter remain production work.
- Candidate fairness does not replace per-peer, per-sender or API rate limits.

## Consequences

Retail payments receive most candidate-verification capacity during mixed
saturation, critical internal work retains a bounded share, and standard traffic
cannot be completely starved. Unused capacity remains work-conserving. Local
metadata cannot change canonical runtime input ordering or ledger validity.

Tests cover saturated per-class allocation, unused-capacity borrowing,
canonical output ordering, invalid/starving policy rejection, trusted promotion,
peer downgrade resistance, gateway retail classification and block-producer
policy bounds.
