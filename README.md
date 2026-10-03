# Payrail Node

Payrail Node is a self-hosted public replica of the Payrail development network.
Anyone can run it. The node follows configured Payrail upstreams, independently
re-executes every sequential finalized block and stores its own durable LMDB
ledger and receipt index.

> **Development network only.** The current upstream uses four equal-weight
> validators and strict three-of-four Ed25519 finality. This node verifies every
> certificate and re-executes every finalized transition independently. It is a
> non-voting replica, not a claim of permissionless production BFT finality.

## Run a public node

Docker Engine with Compose v2 is the shortest path:

```sh
git clone https://github.com/payrail-one/node.git
cd node
docker compose up --build --detach
curl --fail http://127.0.0.1:18080/api/status
curl --fail http://127.0.0.1:18080/health/ready
```

The API is bound to loopback by default. The node follows
`https://devnet.payrail.one` and retains verified ledger data in the
`node-state` Docker volume across restarts. Configure several comma-separated
origins with `PAYRAIL_NODE_UPSTREAMS` for failover.

Set `PAYRAIL_NODE_VALIDATOR_PUBLIC_KEYS` to the canonical comma-separated
four-key devnet validator set published in
`config/devnet-validator-public-keys.txt`. The node fails closed if the set is
absent, malformed or does not match the upstream genesis and certificates.

A hosted demonstration of the same public-replica build is available at
[public-node.payrail.one](https://public-node.payrail.one).

```sh
docker compose logs --follow node
docker compose down
```

See [the operator guide](docs/PUBLIC_NODE.md) before exposing an endpoint or
deleting the state volume.

## API surface

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/api/status` | Network height, finality mode and identity |
| `GET` | `/health/live` | Process liveness |
| `GET` | `/health/ready` | Successful upstream synchronization |
| `GET` | `/api/accounts/{address}` | Finalized balance and nonce |
| `POST` | `/api/transactions` | Submit a canonical signed envelope |
| `GET` | `/api/explorer` | Finalized blocks and transaction summaries |

Reads come from the locally synchronized database. Signed transaction envelopes
are relayed to the primary upstream and later appear locally through the same
verified synchronization path. The public-node API deliberately excludes faucet
and checkout-administration routes.

Wallet signing and canonical envelope construction are intentionally outside
the node trust boundary. Private keys must never be sent to these endpoints.

## Develop and verify

Rust 1.88 is pinned by `rust-toolchain.toml`.

```sh
cargo run --locked -p payrail-node
bash scripts/quality.sh
```

The workspace keeps ledger, authorization, persistence and indexing behind
explicit crate boundaries. Architectural and security decisions are recorded
under `docs/`.

The hosted operator console is versioned in `web/` and uses the same shared
Payrail UI primitives as the wallet and explorer:

```sh
cd web
npm ci --ignore-scripts
npm run check
npm run build
```

## Live development network

[Payrail](https://payrail.one) · [Wallet](https://wallet.payrail.one) ·
[Explorer](https://explorer.payrail.one) · [Devnet](https://devnet.payrail.one)

## Security and license

Please follow [SECURITY.md](SECURITY.md) for vulnerability reports. The source
is licensed under [Apache License 2.0](LICENSE), so anyone may run, inspect,
modify and redistribute the public node under its terms.
