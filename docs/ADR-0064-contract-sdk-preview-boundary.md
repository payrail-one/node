# ADR-0064: Contract SDK preview boundary

## Status

Superseded by [ADR-0065](ADR-0065-payrail-contract-runtime-v1.md).

## Context

Payrail needs a developer-facing surface for programmable payment use cases,
but the current public devnet executes canonical signed payments rather than
smart contracts. ADR-0020 deliberately leaves contract VM selection,
deterministic metering, sandboxing, ABI and upgrade policy unresolved.

The R1 network is a separate system operated by Richard. Payrail currently
integrates with it only through the lottery ticket settlement adapter. Its RPC,
VM and contract APIs are not Payrail infrastructure and must not be presented
as the Payrail contract layer.

## Decision

- Add `@payrail/contracts-sdk` as a `0.0.1-preview` TypeScript package.
- Keep the package transport-neutral behind a `ContractProvider` interface.
- Validate opaque identifiers, entrypoints, payload bounds, integer-only
  monetary values, execution budgets, nonces and expiry heights before invoking
  a provider.
- Do not define consensus-sensitive encoding, signing or contract addresses in
  the preview package.
- `dapp.payrail.one` may demonstrate local intent construction and simulation,
  but must label it as local preview and must not claim a devnet submission or
  finalized contract receipt.
- The dApp may read the real Payrail payment-devnet status only from
  `devnet.payrail.one`; it does not proxy R1 contract or RPC endpoints.

## Production gates

A superseding accepted ADR is required before a live provider is added. It must
define and test:

1. deterministic execution and resource accounting;
2. sandbox and host-function capabilities;
3. canonical ABI, address and state encoding;
4. authorization, replay, expiry and fee semantics;
5. deployment, upgrade, pause and destruction authority;
6. atomic failure behavior and finalized receipt indexing;
7. compatibility and migration rules;
8. Linux devnet evidence for restart, overflow and adversarial inputs.

## Consequences

Product and SDK work can explore a stable application boundary without
silently adopting an external chain. The provider seam is useful for local
examples and future adapters, while the preview label prevents the UI from
overstating implemented network behavior.

The package is not an npm availability promise. Distribution and versioning are
separate release decisions after the execution interface is accepted.

