# ADR-0007: External-asset settlement state machine

Status: accepted for the reference connector boundary; chain adapters and
custody remain separate audited components

## Context

Representing BTC, ETH, stablecoins or tokenized deposits requires more than an
asset ticker. The platform must bind each representation to an exact origin
network and asset, prevent one deposit from minting twice, require a verified
burn before releasing origin assets and continuously reconcile custody backing
against represented supply. Origin-chain finality, RPC parsing and custody keys
have different risks and must not be hidden inside the monetary ledger.

## Decision

1. `external-asset-core` is a deterministic state machine with no network,
   database, RPC or private-key access.
2. Every platform `AssetId` maps to exactly one `(origin network, origin asset)`
   pair. The same origin pair cannot be registered twice.
3. A deposit is identified from the origin network, exact asset, transaction ID
   and event/output index. Amount and recipient conflicts for the same origin
   event stop processing instead of silently changing the claim.
4. Deposit progression is monotonic: `Observed -> Finalized -> MintAuthorized
   -> Minted`. Chain-specific verification happens outside the core; a separate
   finality authority records the block and proof hashes it verified.
5. Withdrawal progression is monotonic: `BurnVerified -> ReleaseAuthorized ->
   Submitted -> Completed`. No release instruction exists before a unique burn
   reference is recorded.
6. Mint references, burn references and origin release transactions are
   one-time identifiers. Exact retries are idempotent; contradictory retries
   fail closed.
7. Observer, finality, settlement and release roles are explicit. Production
   governance may assign them to threshold services but must not collapse them
   implicitly in application code.
8. Pausing stops new risk. Already authorized or externally submitted work can
   still be recorded so reconciliation does not lose irreversible actions.

## Monetary invariants

For each external asset:

```text
finalized deposits >= minted >= burned >= completed releases
expected backing    = finalized deposits - completed releases
represented supply  = minted - burned
backing surplus     = expected backing - represented supply
```

The state machine checks all arithmetic and never permits a negative implied
backing surplus. Its represented-supply total must later be reconciled against
the ledger's actual supply; neither total is accepted as proof of reserves on
its own.

## Security consequences

- A source-chain parser disagreement becomes an explicit reference conflict.
- Recording a proof hash is audit evidence, not proof verification. Bitcoin,
  EVM and issuer APIs require separate finality adapters and adversarial tests.
- A destination is stored as a fixed commitment. Raw addresses and customer
  data remain in the restricted connector/custody service.
- The release service receives deterministic instructions but custody signing
  remains in HSM/MPC or another approved threshold system.
- There is intentionally no ordinary rollback from `Minted` or `Completed`.
  Reorg or custody incidents require suspension, reconciliation and an explicit
  governed recovery procedure rather than history mutation.

## Rejected alternatives

- Treating ticker text as asset identity: the same symbol exists on multiple
  networks and can refer to unrelated contracts.
- Minting immediately after an RPC observation: an observation is not finality.
- Releasing against a user request before burn evidence: permits unbacked
  withdrawals and replay.
- Allowing a connector to hold validator or custody private keys: combines
  compromise domains and defeats role separation.
