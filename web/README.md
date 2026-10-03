# Payrail public node web console

The operator-facing console and read-only API proxy published at
[public-node.payrail.one](https://public-node.payrail.one).

The browser surface reports replica health, synchronization and independently
queryable Payrail devnet state. Transaction signing remains in the Payrail
wallet; the public node never receives wallet secrets.

```sh
npm ci
npm run check
npm run build
```

Production deployment is performed through the pinned Cloudflare Worker in
`apps/node/wrangler.jsonc`.
