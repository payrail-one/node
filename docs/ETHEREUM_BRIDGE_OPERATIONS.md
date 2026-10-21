# Ethereum bridge operations and key inventory

The v1 bridge is federated. Sepolia validation is allowed only after the roles
below have distinct identities and the custody roles are moved off developer
machines. Mainnet remains blocked by ADR-0068's production gates.

## Required identities and secrets

| Role | Required material | Storage and separation |
| --- | --- | --- |
| Vault owner | Ethereum multisig address and its independent signer keys | Hardware wallets or an approved threshold service; never the observer host |
| Vault release authority | A different Ethereum multisig/MPC address and signer set | Separate people/policy from the owner; withdrawal limits and alerts required |
| Testnet deployer | One funded Sepolia EOA | Temporary deployment host or hardware wallet; transfer ownership after deployment and retain no custody authority |
| Payrail registry authority | Payrail signing key and public `AccountId` | Offline/HSM governance boundary |
| Payrail observer authority | Payrail signing key and public `AccountId` | Independent from finality, settlement and release; not loaded by the read-only observer executable |
| Payrail finality authority | Payrail signing key and public `AccountId` | Independent finality attestation boundary |
| Payrail settlement authority | Payrail signing key and public `AccountId` | HSM/remote signer; cannot authorize a user burn by itself |
| Payrail release authority | Payrail signing key and public `AccountId` | Separate from the Ethereum custody signer set even if the same organization operates both |
| User withdrawal | The user's normal Payrail wallet key | Signs the full network, asset, amount, destination, replay reference and expiry before burn |

The observer also needs credentials for at least two independently operated
Ethereum RPC endpoints. API tokens are secrets, but they are not signing keys.
The configured 32-byte provider IDs are stable public operational identifiers,
not keys and not hashes of endpoint URLs.

No private key belongs in the repository `.env`, a command line, process logs,
the LMDB observer journal or the Solidity build. Production services must load
secrets from the approved secret store or HSM/remote-signer boundary. Never
paste key material into an issue, chat or deployment transcript.

## Observer and deposit-settler configuration

The observer's continuous `run` command is read-only. The separate
`settle-deposits` command reads three Payrail testnet authority keys and submits
only deposits already present in the finalized LMDB journal. Install the shared
environment as a root-owned `0600` file at
`/etc/payrail/ethereum-bridge-observer.env`, based on
[`deploy/ethereum-bridge-observer.env.example`](../deploy/ethereum-bridge-observer.env.example).
Install the binary as `/usr/local/bin/ethereum-bridge-observer`, create the
service-owned database directory, and use the hardened example unit
[`deploy/payrail-ethereum-bridge-observer.service`](../deploy/payrail-ethereum-bridge-observer.service).
Enable the oneshot
[`deploy/payrail-ethereum-bridge-settler.service`](../deploy/payrail-ethereum-bridge-settler.service)
through its timer only after the gateway authorities match the key-derived
public IDs. The cursor is written atomically after gateway acceptance, so an
interrupted or repeated run cannot mint the same deposit twice.

Combining the three hot authority keys is permitted only on the isolated R1
testnet. Production must replace them with separate HSM/remote-signer policy
boundaries; the Ethereum custody/release key never belongs in this process.

Before the first start, record the deployed contract address, deployment block
and the exact parent hash of that block. Changing any scope value requires a new
empty journal directory and an explicit migration/reconciliation decision; do
not repoint an existing journal.

## Sepolia ceremony

1. Build and test the exact reviewed source and verify the source, ABI and
   creation-bytecode hashes in the contract README. Use the pinned commands in
   `contracts/ethereum`; do not install an unpinned global deploy tool.
2. Generate the temporary testnet identities with `npm run wallet:testnet --
   <owner-only-key-file>`. Keep deployer, owner and release authority separate.
3. Fund only the temporary deployer with Sepolia ETH. Deploy with `npm run
   deploy:sepolia`; the script pins chain ID `11155111`, waits for two
   confirmations, verifies runtime code and writes immutable deployment
   evidence. Use the governance address as `initialOwner` and the distinct
   release address as `initialReleaseAuthority`.
4. Generate the observer environment with `npm run
   observer-config:sepolia`. The script requires two to eight distinct HTTPS
   RPC endpoints to agree on the recorded deployment block and parent hash.
5. Verify deployed runtime bytecode and constructor arguments independently.
6. Allowlist only the reviewed Sepolia token contracts. Native ETH needs no
   allowlist entry.
7. Configure at least two independent RPC operators and quorum `2`; run
   `scan-once` before enabling the supervised loop.
8. Exercise deposit, finality, mint, owner-signed burn, release, restart, exact
   replay and reconciliation. A journaled deposit is not mint authorization.
9. Test pause, signer rotation, RPC disagreement and incident escalation before
   leaving test funds in custody.

For the current R1 testnet, the observer and periodic deposit settler are
sidecars on `c1-gpu04`; they do not replace or start a second R1 consensus
instance. Build the debug candidate in an isolated directory on that Linux
host, install the binary and units, run `scan-once`, and only then enable the
supervised loop and settlement timer.

The current repository supplies the token vault, quorum observer, durable
journal, authenticated bridge operations, canonical combined state and atomic
settlement transition. The Sepolia tUSDT vault and the verified deposit evidence
are recorded in `contracts/ethereum/deployments/sepolia-usdt.json`. On
2026-10-21, the finalized Sepolia deposit of `100000000` atomic units was
accepted at Payrail height `71` and the same balance was independently read
from all four quorum nodes. Consensus ingestion/execution and automated deposit
settlement are therefore operational on the R1 testnet. Automated Ethereum
release and finalized release completion remain a production gate for the
outbound direction.
