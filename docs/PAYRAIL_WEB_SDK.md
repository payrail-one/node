# Payrail Web SDK

Strict TypeScript packages for Payrail browser and merchant applications:

- `api-client` — typed devnet API and receipt models;
- `money` — canonical decimal-string and checked atomic-unit conversion;
- `wallet-core` — browser-side Ed25519 signing and encrypted local vaults.

Private keys remain client-side. Monetary values cross JSON boundaries as
canonical decimal strings and are represented internally with `bigint`, never
JavaScript `number`.
