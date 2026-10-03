# ADR-0001: Consortium validation with broadly accessible payments

Status: accepted for the prototype

## Decision

The first network prototype will not assume a permissionless validator
set. Consensus participation and payment access are separate concerns.

The initial target topology is:

- permissioned validators operated by independent consortium members;
- private validator signing and consensus RPC;
- public or partner-facing transaction submission through controlled API nodes;
- broadly available wallets and merchant payment interfaces;
- a public explorer and enough non-PII data to verify supply and finality;
- documented read/broadcast interfaces for exchanges and custodians;
- allowlisted runtime upgrades, assets and smart-contract deployment.

Country deployments may restrict read and transaction access further. The core
ledger must therefore remain independent of network-access policy.

## Why

This model supports deterministic finality, regulated operation and clear
validator accountability while still allowing the network token to be held,
traded and used for payments. A traded token does not require anonymous
validator admission. Conversely, a completely opaque network would make supply,
custody and exchange integration harder to verify.

## Future gate

Opening validator admission requires a separate security, legal, economic and
governance review. No code in the ledger crate may assume that the current
permission model is permanent.
