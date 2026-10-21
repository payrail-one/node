# Payrail Wallet

The Payrail self-custody development wallet. It creates an Ed25519 key in the
browser, encrypts the private key locally, signs canonical Payrail operations
and waits for indexed finality.

The demo includes finalized balances, account state, review-before-signing
transfers, encrypted backup and restore, automatic lock, recent activity and one
immutable payment request shared across hosted web, QR and SMS channels. The SMS
action opens the user's device composer; provider-operated outbound delivery is
an independent server-side integration and is not simulated by the wallet.
Test assets have no monetary value.

```sh
cd platform/web
npm ci
npm run check --workspace @platform/payrail-wallet
npm run build --workspace @platform/payrail-wallet
```

Production signing remains client-side or behind an explicitly authorized
hardware/custody boundary. Never send wallet secrets to SSR, logs or analytics.
