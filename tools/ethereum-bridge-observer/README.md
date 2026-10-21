# Ethereum bridge observer

This process scans exactly one configured Ethereum bridge contract. It requires
independent RPC-provider agreement on the complete ordered deposit-log set and
on Ethereum's finalized canonical block, then atomically journals the resulting
Payrail claims in LMDB.

It does not hold Ethereum custody keys, sign releases or mint Payrail assets.
Journaled claims must still pass the consensus settlement boundary described in
[ADR-0068](../../docs/ADR-0068-ethereum-bridge-v1.md).
The required role/key inventory and testnet ceremony are documented in the
[operations runbook](../../docs/ETHEREUM_BRIDGE_OPERATIONS.md).

## Configuration

Configuration is read from the process environment. Do not put production
credentials in repository files or command-line arguments.

```text
PAYRAIL_ETH_CHAIN_ID=11155111
PAYRAIL_ETH_BRIDGE_ADDRESS=0x<40 lowercase hex digits>
PAYRAIL_ETH_FIRST_BLOCK=<decimal deployment block>
PAYRAIL_ETH_FIRST_PARENT_HASH=0x<64 lowercase hex digits>
PAYRAIL_ETH_RPC_PROVIDERS=0x<provider-id-1>=https://...,0x<provider-id-2>=https://...
PAYRAIL_ETH_QUORUM=2
PAYRAIL_ETH_ASSETS=native=0x<payrail-asset-id>,0x<erc20-address>=0x<payrail-asset-id>
PAYRAIL_ETH_OBSERVER_DB=/absolute/service-owned/path
PAYRAIL_ETH_OBSERVER_MAP_SIZE=67108864
PAYRAIL_ETH_POLL_MILLIS=3000
```

Provider IDs are stable 32-byte operational identities, not hashes of URLs.
Use providers run by independent operators. URL query credentials are accepted
but redacted from errors and debug output.

Run one step during validation:

```sh
cargo run -p ethereum-bridge-observer -- scan-once
```

Run continuously under a process supervisor:

```sh
cargo run -p ethereum-bridge-observer -- run
```

Only one process may own a given LMDB path. A disagreement, malformed deposit,
fork or scope mismatch stops the process for operator investigation; temporary
lack of quorum or finality is retried in daemon mode.
