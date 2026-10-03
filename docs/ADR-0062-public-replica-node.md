# ADR-0062: Public replica node

## Status

Accepted for the single-node Payrail development network.

## Context

The first public build started an isolated ledger. Two installations therefore
created two unrelated histories. A public node must instead follow the public
Payrail history and maintain its own independently executed local copy.

## Decision

1. `payrail-node` is a public replica available to every operator. It polls up
   to eight credential-free HTTP(S) upstream origins, fails over between them
   and stores its own LMDB ledger and receipt index.
2. Every imported block must match the compiled network and genesis, advance by
   exactly one height, name the local parent, pass the configured development
   finality boundary, reproduce the block hash and state root under deterministic
   execution, and commit state, payload and cursor atomically.
3. Startup reopens and validates durable history. Synchronization never deletes
   or rewinds local state when an upstream reports a lower or conflicting tip.
4. Public APIs expose local finalized reads, liveness/readiness and relay
   client-signed envelopes. Faucet and checkout-administration routes are not
   exposed. POST submission uses one primary upstream and is not blindly
   retried after an ambiguous transport result.

## Security boundary

The current development proof is still the explicit `single-node-devnet`
marker. The replica independently checks transition integrity and detects forks
relative to its durable cursor, but the upstream authority can still choose an
alternative valid history. Production promotion requires real Payrail validator
certificates, authenticated validator-set transitions and a signed bootstrap
anchor. R1 is an external settlement integration and is not a Payrail node
upstream.
