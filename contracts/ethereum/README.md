# Payrail Ethereum bridge contracts

`PayrailTokenVault.sol` is the single-token custody contract used by the
current explicitly federated Ethereum bridge. It accepts exact-transfer ERC-20
deposits, emits a Payrail account commitment, and releases each finalized
Payrail withdrawal identifier at most once. `PayrailBridgeVault.sol` remains
the earlier multi-asset/native-ETH candidate and is not the active USDT path.

The reviewed candidate is deployed on Sepolia for integration testing. It is
not an audited or mainnet-approved contract. Do not fund it on mainnet until all gates in
[ADR-0068](../../docs/ADR-0068-ethereum-bridge-v1.md) are complete.

## Trust boundary

- `owner` can pause deposits/releases, rotate the release authority and manage
  ERC-20 admission for new deposits. Removing admission never blocks release
  of assets already held by the vault.
- `releaseAuthority` can release custody assets. Production must assign this to
  a separately governed multisig or threshold signer, never a hot key in the
  observer process.
- Payrail minting is driven only by the Rust adapter after the emitted log's
  exact block hash is established as canonical and finalized.
- Fee-on-transfer, rebasing, callback-dependent and otherwise non-standard
  tokens are rejected unless separately reviewed and explicitly supported.

The pinned compiler target is Solidity `0.8.24`. No imported Solidity package
or upgradeable proxy is used in this first candidate. Compiler binaries, build
settings, bytecode hashes, ABI, tests, fuzzing output and audit evidence must be
committed before a deployment is approved.

The executable local-EVM suite pins `hardhat 3.20.0`; its configuration pins
`solc 0.8.24` and uses Hardhat's built-in Solidity test runner. Run it with
`npm ci --ignore-scripts && npm test`. It covers native/ERC-20 deposits and
releases, role separation, allowlisting, pause, replay, direct-transfer
rejection, fee-on-transfer rejection and rollback, including 2,000 fuzz cases
for each deposit path in the CI profile. The committed lockfile is the
dependency inventory for this candidate; `npm audit` reported no known
vulnerabilities on 2026-10-21. Hardhat and its resolved dependencies remain
development-only and are not part of the deployed bytecode.

The current source compiles with:

```sh
npx --yes solc@0.8.24 --optimize --optimize-runs 200 \
  --bin --abi contracts/ethereum/src/PayrailBridgeVault.sol -o <output-directory>
```

Local verification on 2026-10-21 produced SHA-256
`e54fe37f6721246139c36bdf8084c6a1aa7aac4d57a16eb9269c6b7ed2bfa17f`
for the candidate creation bytecode and
`2598fbaea45e8af84f3534da85dc9e622a7c20a9de14ad225e2166eb862b7638`
for its ABI. The corresponding source SHA-256 is
`25bb91f3c3e964c536ed1c7b8c105b17744f11205f72612e67d0dca5ae7e8e0b`.
These are review evidence only, not an approved deployment.

## Sepolia deployment

Testnet keys are read from owner-only files and are never accepted directly in
environment variables or command-line arguments. Generate separate deployer,
owner and release-authority wallets outside the repository, fund only the
deployer with Sepolia ETH, then compile and deploy:

```sh
npm run wallet:testnet -- /secure/path/deployer.key
npm run wallet:testnet -- /secure/path/owner.key
npm run wallet:testnet -- /secure/path/release-authority.key
npm run compile

PAYRAIL_SEPOLIA_RPC_URL=https://... \
PAYRAIL_SEPOLIA_DEPLOYER_KEY_FILE=/secure/path/deployer.key \
PAYRAIL_SEPOLIA_OWNER_ADDRESS=0x... \
PAYRAIL_SEPOLIA_RELEASE_AUTHORITY_ADDRESS=0x... \
PAYRAIL_SEPOLIA_DEPLOYMENT_FILE=deployments/sepolia.local.json \
npm run deploy:sepolia
```

The deploy command refuses non-Sepolia RPC endpoints, zero or duplicated role
addresses, group-readable key files, unfunded deployers and failed or codeless
deployments. It waits for two confirmations and writes immutable public
deployment evidence. Generate the private observer environment only after that
evidence exists and at least two RPC providers agree on its block:

```sh
PAYRAIL_SEPOLIA_DEPLOYMENT_FILE=deployments/sepolia.local.json \
PAYRAIL_SEPOLIA_OBSERVER_ENV_FILE=deployments/sepolia.local.env \
PAYRAIL_SEPOLIA_RPC_URLS=https://provider-1.example,https://provider-2.example \
npm run observer-config:sepolia
```

Both `.local.json` and `.local.env` deployment outputs are ignored. Copy the
observer environment to the designated R1 host with mode `0600`; never copy a
custody or release key into the observer service.

### Current Sepolia tUSDT deployment

Sepolia has no official Tether-issued USDt. `PayrailTestUsdt` is an unbacked,
testnet-only six-decimal token used to validate the same no-return transfer ABI
as Ethereum mainnet USDt. It must never be represented as real USDt.

- tUSDT token: [`0x5e3a1bBB81d5C3c1391796E4e3c8c4E0420a8c4a`](https://sepolia.etherscan.io/address/0x5e3a1bBB81d5C3c1391796E4e3c8c4E0420a8c4a)
- Token vault: [`0x74E345a36BCf50B253bac512C322d1a8Abc6b298`](https://sepolia.etherscan.io/address/0x74E345a36BCf50B253bac512C322d1a8Abc6b298)
- Vault deployment transaction: [`0xb118ea9a3b2e9a6e1cd90cdd8839fae7e0daef86eff886d81ca9d8560ec1b9cb`](https://sepolia.etherscan.io/tx/0xb118ea9a3b2e9a6e1cd90cdd8839fae7e0daef86eff886d81ca9d8560ec1b9cb)
- Verified 100 tUSDT deposit: [`0xf3a74c319d3005b8d2c8e66d6e10e997550d72342613a09060066f316766bd12`](https://sepolia.etherscan.io/tx/0xf3a74c319d3005b8d2c8e66d6e10e997550d72342613a09060066f316766bd12)
- Payrail bridge asset ID:
  `0xb3567d3b1507f7b20c31ea7a48aa92e16d808a77674bef638b97ecaa909fc90b`
- R1 result: finalized height `71`, recipient balance `100000000`
  atomic units on all four bridge quorum nodes.

The immutable public deployment and end-to-end deposit evidence is committed
in [`deployments/sepolia-usdt.json`](deployments/sepolia-usdt.json). Local key
files, RPC credentials and generated observer environments are excluded.

### Legacy native-ETH deployment

- Vault: [`0xc4f167f0df1E25c3B20cBf0f72f48A229aEa1A47`](https://sepolia.etherscan.io/address/0xc4f167f0df1E25c3B20cBf0f72f48A229aEa1A47)
- Deployment transaction: [`0x6975a657f3a7bee82e401c75e4e380091316389c7b3df75cb0a0056e20ac380c`](https://sepolia.etherscan.io/tx/0x6975a657f3a7bee82e401c75e4e380091316389c7b3df75cb0a0056e20ac380c)
- Deployment block: `11962945`
- Native Sepolia ETH Payrail asset ID:
  `0xa145ec56fb0034aa0eb2cee09dc782d5c2e97517ff6a9219e7d3bff0aa7ee07d`

Its immutable public record, including the block lineage, role addresses and
artifact hashes, is committed in
[`deployments/sepolia.json`](deployments/sepolia.json). Local key files and the
observer environment are deliberately excluded.
