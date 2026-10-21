# ADR-0068: Federated Ethereum bridge v1 boundary

## Status

Accepted for implementation and isolated testnet validation. Mainnet custody,
deployment and production minting are not approved.

## Context

Payrail needs to represent native ETH and selected ERC-20 assets without
allowing an Ethereum observation, reorg, duplicated log or compromised relayer
to create unbacked Payrail supply. The generic lifecycle and reconciliation
rules already exist in `external-asset-core`, but an Ethereum log is not proof
of finality and the current Payrail network does not expose a proof that an EVM
contract can trustlessly verify.

Calling this bridge trustless would therefore misstate the security model. A
first useful bridge can instead make custody and finality authorities explicit,
keep them separate, and preserve a migration path to verified light-client
proofs.

## Decision

1. Ethereum bridge v1 uses a lock/mint and burn/release model governed by the
   state machine in [ADR-0007](ADR-0007-external-asset-settlement.md).
2. The first deployment is federated. The vault release authority must be a
   separately governed multisig or threshold account. Observer, Ethereum
   finality, Payrail settlement and custody release roles are not silently
   collapsed into one service or key.
3. The bridge vault emits
   `Deposited(bytes32,address,uint256,uint256)`. The indexed `bytes32` value is
   a Payrail `AccountId`, the indexed address is the exact ERC-20 contract or
   the zero-address native-ETH sentinel, and amounts are integer atomic units.
4. Each asset binding includes the EVM chain ID, bridge contract, exact origin
   asset and Payrail `AssetId`. Native ETH and every ERC-20 contract derive
   distinct origin identifiers. Symbols are display metadata only.
5. A deposit identity is the Ethereum transaction hash and log index. The
   adapter rejects removed logs, missing receipt metadata, wrong contracts,
   event/topic ambiguity, non-canonical address padding, zero amounts and EVM
   `uint256` amounts that do not fit Payrail's checked `u128` balance type.
6. Deposit acceptance requires the log's block hash to equal the canonical
   block hash at that exact height under the configured Ethereum finality
   policy. `latest`, `safe`, a confirmation count, and receipt success alone are
   insufficient. The verifier records a non-zero commitment to its independently
   checked finality evidence.
   That commitment binds the chain, block and fixed provider-quorum policy; it
   does not change merely because a different allowed subset of providers was
   reachable during an exact retry.
   The complete ordered deposit-log set, block hash and parent hash also require
   agreement from the independently identified RPC-provider quorum. A single
   provider is not trusted to report log contents merely because the block hash
   is canonical.
7. Releases are keyed by the canonical Payrail withdrawal ID. The vault marks
   the ID before the external transfer and rejects every replay. A reverted
   transfer reverts the replay mark atomically.
8. Token admission is allowlisted. The initial contract requires exact balance
   changes and rejects fee-on-transfer behavior. Rebasing, callback-dependent
   and non-standard tokens need a separate decision and tests.
9. Pausing stops new deposits and releases. Ownership transfer is two-step.
   Direct ETH transfers are rejected so every accepted deposit has a recipient
   event.

## Atomic integration requirement

Ethereum evidence, `external-asset-core` progression, Payrail backing updates,
ledger mint/burn and finalized references must be committed as one recoverable
application transition or through a journaled protocol that proves equivalent
failure atomicity. The pure EVM adapter does not by itself authorize a mint.
Production enablement remains blocked until this integration is part of the
authenticated consensus state and survives restart/replay testing.

The off-chain observer uses a separate scope-bound LMDB journal. A finalized
block, every verified deposit in that block and the next expected parent are
one transaction. The journal retains only the last block commitment plus the
deposit records, rather than growing one empty record for every Ethereum block.
Opening LMDB uses the same single reviewed `heed` memory-map boundary accepted
for finalized Payrail storage in
[ADR-0019](ADR-0019-lmdb-finalized-state-store.md); mapped references remain
transaction-bound and no additional unsafe code is permitted.

`external-asset-settlement-core` supplies the in-memory atomic boundary between
the ledger and external-asset state machines: registration, finalized deposit
and mint, burn and withdrawal creation, release submission, release completion
and verified-backing reduction are clone/validate/commit transitions. Exact
retries are idempotent and every transition cross-reconciles represented ledger
supply and verified backing against the external lifecycle totals.
`external-asset-runtime-core` provides a single bounded canonical snapshot
framing for both component states and validates that reconciliation again on
encode and restart. These primitives satisfy the state and restart portion of
the requirement. User burns additionally require a process-local capability
created only after verification of the account owner's network-bound signature
over the asset, amount, destination, unique burn reference and expiry. They do
now enter the same authenticated external-settlement block codec used by the
deployed debug R1 gateway. A four-validator certificate and the combined state
commit are required before the gateway reports success.

## Security consequences

- Compromise of the release authority can drain vault custody. Separation,
  threshold policy, withdrawal limits, monitoring and an emergency pause reduce
  but do not eliminate this federated trust assumption.
- A compromised observer cannot mint from an arbitrary or unfinalized log when
  finality and settlement roles remain independent. Collusion of the configured
  authorities remains a v1 trust risk.
- A single unverified RPC response is not finality evidence. The infrastructure
  adapter uses independently operated provider quorums for both finality and
  complete block-log observation, with fail-closed disagreement. A verified
  Ethereum light-client source may replace this boundary later. Evidence does
  not store endpoint credentials or raw RPC payloads.
- Contract upgrades are not implicit. A new vault address is a governed binding
  change with an overlap/reconciliation plan and separate audit evidence.

## Production gates

- compile with a pinned, reproducible Solidity toolchain and commit ABI,
  bytecode, settings, source hash and dependency/license inventory;
- unit, fuzz and invariant tests for deposit/release success, authorization,
  pause, replay, reentrancy, zero/boundary values and failing token behavior;
- independent contract and bridge-protocol security review;
- durable EVM cursor/reorg recovery and at least two independent finality data
  paths with fail-closed disagreement handling;
- atomic Payrail consensus-state integration with restart and replay tests;
- automated Ethereum submission and finalized completion of owner-signed
  burn/withdrawal operations;
- HSM/MPC or multisig release runbook, limits, alerting and incident drill;
- isolated Sepolia end-to-end tests covering deposit through Payrail finality
  and Payrail burn through finalized Ethereum release;
- continuous reconciliation of vault balances, finalized deposits, represented
  supply, burns and completed releases.

## Verification evidence

`ethereum-bridge-core` provides dependency-light deterministic event decoding,
origin identity derivation, independent finality and full-block observation
quorums, end-to-end ingestion preparation and finalized-block sequencing.
`ethereum-bridge-rpc` provides a redirect-free HTTPS adapter with chain-ID
binding, strict canonical JSON-RPC decoding, bounded response bodies and no
credential-bearing errors. `ethereum-bridge-observer-lmdb` atomically persists
the scope-bound cursor and verified deposits. `tools/ethereum-bridge-observer`
is the runnable, environment-configured process that composes both quorums with
that journal. Its continuous observer command contains no signing key; its
separately supervised testnet settler loads role-separated files and signs only
journaled deposits. It contains no Ethereum custody or release key. Their
tests cover native and
ERC-20 success, exact retry, conflicting replay, wrong authority boundary,
malformed and overflowing data, incomplete receipts, provider disagreement,
stable evidence, unknown and ambiguous assets, gaps, forks, restart,
pagination, map exhaustion and failure atomicity.

`external-asset-core` now has a bounded canonical snapshot codec that rebuilds
and validates every replay index and monetary total. The settlement and runtime
crates add atomic ledger/external transitions, combined restart state and
cross-reconciliation tests, including late ledger failure rollback and exact
retry without duplicate mint or burn. The withdrawal boundary verifies a
network/domain-separated owner signature, binds destination and expiry, and
checks expiry again at burn execution. Consensus transport and execution of
these authenticated capabilities is covered by the combined runtime, restart
tests and the four-node Sepolia deposit proof at Payrail height `71`. The
outbound EVM release worker remains a production gate.

The new network dependencies are pinned to `reqwest 0.13.2` and `rustls
0.23.45`; the Rustls `ring` provider is selected explicitly to avoid a second
cryptographic provider in this workspace. Their relevant dependency licenses
are permissive MIT, Apache-2.0 and/or ISC combinations. This review is build
evidence, not a substitute for the production SBOM gate above.

`contracts/ethereum/src/PayrailTokenVault.sol` is the active unaudited
single-token candidate for tUSDT on Sepolia and official USDt on Ethereum
mainnet after the production gates. The pinned Hardhat 3.20.0 suite compiles it
with Solidity 0.8.24 and covers authorization, pause, replay, direct-transfer
rejection, exact no-return ERC-20 deltas and rollback. The 2026-10-21 npm audit
reported zero known dependency vulnerabilities. The testnet deployment is not
mainnet approval or an independent security review.

## Rejected shortcuts

- Mint after an RPC reports a successful receipt: receipts can be observed on
  a non-final or non-canonical fork.
- Use token symbols as identities: symbols do not bind a network or contract.
- Put custody keys in the observer or gateway: this collapses compromise
  domains and bypasses release governance.
- Advertise v1 as trustless: Ethereum cannot yet verify Payrail finality, and
  Payrail does not yet run an Ethereum light client in consensus.
- Support arbitrary ERC-20 contracts: token behavior can invalidate exact
  backing and release assumptions.
