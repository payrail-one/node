# Payrail Network

Payrail is payment infrastructure for every channel: one authorized payment,
one deterministic final receipt. The network core is written in Rust and keeps
money, authorization, execution, persistence, indexing and transport behind
explicit boundaries.

The current public environment is a development network. It provides real
client-side Ed25519 authorization, integer-only balances, nonce-based replay
protection, atomic ledger execution, durable LMDB state and an independently
maintained finalized-receipt index. Test assets have no monetary value.

## Public surfaces

- `https://payrail.one` — product site;
- `https://wallet.payrail.one` — encrypted self-custody test wallet;
- `https://explorer.payrail.one` — independent ledger explorer;
- `https://devnet.payrail.one` — live network dashboard.

## Trust boundary

Private keys remain in the browser and are encrypted with AES-256-GCM using a
PBKDF2-derived key. Browser views are presentation clients; balances, nonces,
execution outcomes and finality come from the ledger and its independent index.
The public devnet uses four independently persisted validators and strict
three-of-four Ed25519 finality. Each voter independently executes the proposal
before signing, and replicas reverify certificates before applying state. This
is a fixed-coordinator development quorum, not permissionless production BFT.

## Engineering

The repository uses pinned Rust and TypeScript dependencies, checked integer
amounts and failure-atomic state transitions. See the ADRs under `docs/` for the
security, persistence, wallet, indexing and omnichannel checkout decisions.
