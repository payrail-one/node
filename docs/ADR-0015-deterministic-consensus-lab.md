# ADR-0015: deterministic seven-validator consensus lab

## Status

Accepted as a reproducible fault-model and pre-devnet test tool. It is not a
GRANDPA implementation, production benchmark, or substitute for a remote
multi-host testnet.

## Context

The target network needs low finality latency and rapid node recovery while
keeping validators in independent failure domains. Happy-path unit tests and a
single-host TLS loopback do not expose quorum loss, asymmetric message loss,
partitions, or stale-node recovery.

## Decision

- `tools/consensus-lab` contains exactly seven validators in a configurable
  weighted set and requires at least three failure domains.
- The standard reproducible topology uses a 3/2/2 domain distribution.
- A directed full-mesh model controls intra/inter-domain delay, offline nodes,
  asymmetric link loss and bidirectional partitions.
- A modeled round selects votes by round-trip arrival time and requires the
  existing strictly-greater-than-two-thirds weighted quorum.
- Every successful round creates real Ed25519 signatures, encodes a bounded
  finality certificate and passes it through the existing strict verifier.
- Per-node finalized height is tracked. A node behind a partition advances only
  after connectivity is restored and the explicit catch-up model runs.
- All embedded signing seeds are deterministic lab fixtures and are forbidden
  for deployed validators.

## Limits

The lab abstracts proposer selection, GRANDPA prevote/precommit state machines,
timeouts, equivocation, disk I/O, TLS handshakes and operating-system scheduling.
Reported milliseconds are deterministic model outputs, not measured production
latency or TPS. The next stage must run real node processes over mTLS locally and
then on separate hosts/failure domains with network fault injection.
