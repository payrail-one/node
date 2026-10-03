# Running the Payrail development node

The public Payrail node is a single-process development network for testing the
signed payment, durable-ledger and independently indexed receipt path. It is not
a production validator and does not provide multi-validator BFT finality.

## Start with Docker Compose

From the public node repository:

```sh
docker compose up --build --detach
curl --fail http://127.0.0.1:18080/api/status
```

The API listens on loopback by default and stores ledger state in the managed
`node-state` volume. Inspect or stop it with:

```sh
docker compose logs --follow node
docker compose down
```

`docker compose down` preserves the volume. Treat `docker compose down
--volumes` as destructive because it deletes the local devnet ledger.

## Public exposure

Do not bind the container directly to an internet-facing address. Put a TLS
reverse proxy or API gateway in front of the loopback listener and enforce
request-body limits, rate limits, timeouts and access logs that exclude signed
envelopes and other sensitive request bodies. The repository includes sanitized
systemd and nginx examples under `deploy/`.

The development API includes a test-asset faucet and checkout creation. Test
assets have no monetary value. A production deployment requires separate
authentication, abuse controls, key management, consensus and operational
review.

## Native development

Rust 1.88 is required:

```sh
cargo run --locked -p devnet-gateway
```

The native process defaults to `127.0.0.1:18080` and `.devnet/gateway`. Override
those values with `DEVNET_BIND` and `DEVNET_DATA_DIR`.

## Verification

```sh
bash scripts/check-source-size.sh
cargo fmt --all --check
cargo test --locked -p devnet-gateway
cargo clippy --locked -p devnet-gateway --all-targets -- -D warnings
```
