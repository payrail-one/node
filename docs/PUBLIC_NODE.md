# Running a Payrail public node

`payrail-node` is a public replica that any operator can run. It follows the
Payrail development network, verifies and re-executes sequential blocks, and
keeps its own durable ledger and receipt index. It is not yet a production
validator and the current upstream does not provide multi-validator BFT finality.

## Start with Docker Compose

From the public node repository:

```sh
docker compose up --build --detach
curl --fail http://127.0.0.1:18080/api/status
curl --fail http://127.0.0.1:18080/health/ready
```

The API listens on loopback by default and stores ledger state in the managed
`node-state` volume. Inspect or stop it with:

```sh
docker compose logs --follow node
docker compose down
```

`docker compose down` preserves the volume. Treat `docker compose down
--volumes` as destructive because it deletes the local devnet ledger.

The default upstream is `https://devnet.payrail.one`. Configure up to eight
credential-free origins in failover order:

```sh
PAYRAIL_NODE_UPSTREAMS=https://node-a.example,https://node-b.example \
  docker compose up --build --detach
```

The node rejects a foreign genesis, a lower remote height, gaps, forks,
oversized responses and blocks whose independently computed hash or state root
does not match the announced checkpoint.

## Public exposure

Do not bind the container directly to an internet-facing address. Put a TLS
reverse proxy or API gateway in front of the loopback listener and enforce
request-body limits, rate limits, timeouts and access logs that exclude signed
envelopes and other sensitive request bodies. The repository includes sanitized
systemd and nginx examples under `deploy/`.

The public-node API has no faucet, checkout administration or signing keys.
Test assets have no monetary value. A production deployment still requires
validator certificates, authenticated membership changes, abuse controls and
operational review.

## Native development

Rust 1.88 is required:

```sh
cargo run --locked -p payrail-node
```

The native process defaults to `127.0.0.1:18080` and `.payrail/node`. Override
them with `PAYRAIL_NODE_BIND`, `PAYRAIL_NODE_DATA_DIR`,
`PAYRAIL_NODE_UPSTREAMS` and `PAYRAIL_NODE_SYNC_INTERVAL_SECONDS`.

## Verification

```sh
bash scripts/check-source-size.sh
cargo fmt --all --check
cargo test --locked -p devnet-gateway -p payrail-node
cargo clippy --locked -p devnet-gateway -p payrail-node --all-targets -- -D warnings
```
