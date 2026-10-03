# Payrail Contracts SDK

`@payrail/contracts-sdk` builds deterministic Payrail `PRC1` contracts. Signed
deployments and calls are finalized by the Payrail devnet runtime; this is not
an R1 adapter.

## Contract source

The browser sandbox and SDK accept Payrail Contract Language v1. The compiler
parses a bounded instruction language and never evaluates JavaScript.

```text
payrail 1
entry deposit
  attached_amount
  store deposited
  emit Deposited
end

entry refund
  state deposited
  transfer_caller
  const 0
  store deposited
  emit Refunded
end
```

```ts
import { compileContractSource } from '@payrail/contracts-sdk';

const code = compileContractSource(source);
const envelope = await wallet.signContractDeploy({
  networkId,
  assetId,
  idempotencyKey: crypto.getRandomValues(new Uint8Array(32)),
  salt: crypto.getRandomValues(new Uint8Array(32)),
  code,
  fee: 1n,
  nonce,
  validUntilHeight,
});
await api.submit(envelope);
```

The builder API exposes the same instructions through `contractProgram()` and
`contractBody()`. Values are `bigint`; code, arguments, entrypoints and
execution are checked against runtime limits before signing.

## Runtime limits

| Boundary | Limit |
| --- | ---: |
| Bytecode | 16 KiB |
| Call arguments | 4 KiB |
| Execution | 100,000 instructions |
| Entrypoints | 16 |
| Stack | 64 `u128` values |
| State writes per call | 32 |
| Events per call | 16 |

See [ADR-0065](ADR-0065-payrail-contract-runtime-v1.md) for encoding, atomicity,
state and compatibility decisions.

