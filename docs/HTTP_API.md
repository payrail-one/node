# HTTP API

## Status

This documents the implemented development-network transport. It is not a
stable production API and currently has no versioned compatibility guarantee.
Test assets have no monetary value. Request and response money fields are
decimal strings in atomic units.

The gateway base URL is `/api`. JSON field names are camel case unless the
schema below shows otherwise. Errors use:

```json
{"error":"human-readable development error"}
```

Clients must branch on HTTP status, not parse the error text.

## Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/api/status` | Network identity, finality mode, asset and finalized height |
| `GET` | `/api/accounts/{address}` | Finalized balance and next nonce |
| `POST` | `/api/faucet` | One test-only funding request per address |
| `POST` | `/api/transactions` | Submit one canonical lowercase-hex signed envelope |
| `GET` | `/api/explorer` | Recent finalized blocks and transactions |
| `GET` | `/api/live?address=...` | WebSocket finalized/checkout events filtered by address |
| `POST` | `/api/checkouts` | Create an immutable development checkout |
| `GET` | `/api/checkouts/{id}` | Read checkout presentation and finality state |
| `POST` | `/api/checkouts/{id}/transactions` | Pay checkout with a signed envelope |

## Status and account

`GET /api/status`:

```json
{
  "networkId": "<64 lowercase hex>",
  "addressPrefix": "paydev",
  "finalizedHeight": "12",
  "finalityMode": "single-node-devnet",
  "asset": {"id": "<64 lowercase hex>", "symbol": "TEST", "decimals": 6}
}
```

`GET /api/accounts/{address}`:

```json
{
  "address": "paydev1...",
  "accountId": "<64 lowercase hex>",
  "nonce": "3",
  "balance": "125000000",
  "finalizedHeight": "12"
}
```

The `nonce` is the next finalized sender nonce. Clients must refresh it before
constructing a new operation and must not guess around a pending result.

## Faucet and transaction submission

Faucet request:

```json
{"address":"paydev1..."}
```

Transaction request:

```json
{"envelope":"<canonical lowercase hexadecimal envelope>"}
```

Both return a finalized transaction plus checkpoint:

```json
{
  "transaction": {
    "id": "<64 lowercase hex>",
    "blockHeight": "13",
    "operationIndex": "8",
    "from": "paydev1...",
    "to": "paydev1...",
    "amount": "1000000",
    "fee": "0",
    "outcome": "applied"
  },
  "checkpoint": {
    "height": "13",
    "hash": "<64 lowercase hex>",
    "stateRoot": "<64 lowercase hex>",
    "transactionCount": 1
  }
}
```

An `expired` outcome consumes the exact nonce but moves no value. It must never
be displayed as a successful payment.

## Checkout

Create request:

```json
{
  "merchantAddress": "paydev1...",
  "amount": "2500000",
  "orderReference": "merchant-order-123"
}
```

The response contains an immutable ID, amount, fee, asset, expiry, signed
validity height, payment link/SMS text and optional finalized transaction. The
status is `open`, `processing`, `finalized` or `expired`. Reusing an order
reference with conflicting monetary data fails; the API does not edit the old
checkout.

## Error status map

- `400` — malformed address, hex, envelope, operation, transaction or checkout;
- `404` — checkout not found;
- `409` — faucet already used or checkout claim/conflict;
- `503` — development faucet exhausted;
- `500` — state unavailable or internal invariant failure.

## Public replica node

The replica node intentionally exposes a smaller surface:

- `GET /api/status` — local/upstream heights and synchronization state;
- `GET /api/accounts/{address}` — local finalized account view;
- `GET /api/explorer` — locally indexed finalized history;
- `POST /api/transactions` — relay a signed envelope to one upstream;
- `GET /health/live` and `GET /health/ready` — process and synchronization
  probes.

It does not expose the faucet or checkout-administration routes. A transport
timeout during `POST /api/transactions` is ambiguous and must not trigger blind
resubmission with a different operation.

## TypeScript client

`@platform/api-client` is the canonical browser contract. Use
`@platform/money` for conversion and `@platform/wallet-core` for signing; do not
reimplement envelope encoding or amount parsing in an application.

