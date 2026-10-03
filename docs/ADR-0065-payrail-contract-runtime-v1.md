# ADR-0065: Payrail deterministic contract runtime v1

## Status

Accepted for the public Payrail development network. This decision does not
authorize real-value or production contract deployment.

## Context

The preview SDK deliberately stopped at local simulation because Payrail had no
consensus contract format. A useful developer network must instead put a signed
deployment or call into the canonical block payload, execute it identically on
every validator and commit code, balances and state to the authenticated state
root. R1 is a separate network and is not the Payrail contract runtime.

## Decision

Payrail introduces the versioned `PRC1` deterministic bytecode runtime and two
canonical operations:

- `ContractDeploy` (operation discriminant `4`) carries network, idempotency
  key, asset, owner, 32-byte salt, bounded code, fee, nonce and expiry height;
- `ContractCall` (operation discriminant `5`) carries network, idempotency key,
  asset, caller, contract ID, entrypoint, arguments, attached amount, fee,
  execution limit, nonce and expiry height.

Both use the existing Ed25519 authorization domain and signed-envelope format.
Contract IDs are SHA-256 hashes over a domain separator and the complete
canonical deployment. Replays, wrong networks, nonce gaps and expired
operations use the same authoritative ledger checks as payments.

`PRC1` has no system calls, wall clock, randomness, network, filesystem,
floating point or unbounded allocation. Its instruction set provides checked
`u128` arithmetic, comparisons, requirements, bounded integer state, events and
transfers from the contract account. Execution is limited to 100,000
instructions, 16 KiB of code, 4 KiB of call arguments, 16 entrypoints, 64 stack
items, 32 writes and 16 events per call. Monetary values remain integer atomic
units.

Execution stages all balance transfers, state writes and events. Any validation,
arithmetic, authorization, funding, rejection or resource-limit failure leaves
balances, state, nonce, fees and receipts unchanged. A successful call and its
receipt are committed atomically.

Canonical ledger state encoding uses version 3 when contract rows are present
and adds ordered contract code and contract-state rows. Contract-free state
continues to use byte-identical version 2 encoding, so an in-place devnet
upgrade preserves its existing state root; the first contract deployment moves
the state to version 3. Version 2 remains readable. Code is
immutable in v1: there is no upgrade, pause or destroy instruction. A new
deployment and application-level migration are required.

The TypeScript SDK provides a bytecode builder and a non-evaluating Payrail
Contract Language v1 compiler. Browser signing remains in `wallet-core`; private
keys never reach the gateway. `dapp.payrail.one` may create an ephemeral funded
devnet wallet, sign deploy/call envelopes and show the finalized checkpoint and
queried canonical state.

## Compatibility and future VM versions

The `PRC1` magic is the VM-version boundary. New opcodes cannot change existing
semantics. A WASM or other runtime requires a new magic/version, deterministic
metering specification, adversarial tests and a separate accepted ADR. Unknown
versions and opcodes are rejected before state mutation.

## Consequences

Payrail now has a small auditable execution layer sufficient for real devnet
contracts, with deliberately narrow capabilities. It is not EVM-compatible and
does not claim general-purpose production safety. Production promotion requires
an external security review, validator upgrade procedure, fee-market policy,
capacity evidence and long-running recovery/fuzz testing.

