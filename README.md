# Payrail Node

Payrail Node is the source distribution of the Payrail development network. It
provides a deterministic Rust ledger, client-signed Ed25519 payments, checked
integer balances, nonce replay protection, persistent LMDB state and an
independently maintained finalized-receipt index.

> **Development network only.** This build uses single-node development
> finality and test assets with no monetary value. It is not a production
> validator or a claim of multi-validator BFT finality.

## Run a local node

Docker Engine with Compose v2 is the shortest path:

```sh
git clone https://github.com/payrail-one/node.git
cd node
docker compose up --build --detach
curl --fail http://127.0.0.1:18080/api/status
```

The API is bound to loopback by default. Ledger data is retained in the
`node-state` Docker volume across container restarts.

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
| `GET` | `/api/accounts/{address}` | Finalized balance and nonce |
| `POST` | `/api/transactions` | Submit a canonical signed envelope |
| `GET` | `/api/explorer` | Finalized blocks and transaction summaries |
| `POST` | `/api/faucet` | Claim test-only development assets |
| `POST` | `/api/checkouts` | Create a test merchant checkout |
| `GET` | `/api/checkouts/{id}` | Read checkout state |
| `POST` | `/api/checkouts/{id}/transactions` | Pay a checkout with a signed envelope |

Wallet signing and canonical envelope construction are intentionally outside
the node trust boundary. Private keys must never be sent to these endpoints.

## Develop and verify

Rust 1.88 is pinned by `rust-toolchain.toml`.

```sh
cargo run --locked -p devnet-gateway
bash scripts/quality.sh
```

The workspace keeps ledger, authorization, persistence and indexing behind
explicit crate boundaries. Architectural and security decisions are recorded
under `docs/`.

## Live development network

[Payrail](https://payrail.one) · [Wallet](https://wallet.payrail.one) ·
[Explorer](https://explorer.payrail.one) · [Devnet](https://devnet.payrail.one)

## Security and source terms

Please follow [SECURITY.md](SECURITY.md) for vulnerability reports. The source
is publicly available for inspection but remains `UNLICENSED`; see
[LICENSE](LICENSE). Repository visibility does not grant permission to copy,
modify or redistribute it.
