# Merchant checkout transport contract

## Status and scope

This is a transport-neutral draft for fixed-amount, single-use checkouts. It
defines semantics for the future HTTP/gRPC service without declaring a public
API version. The Rust domain and LMDB adapter implement the state machine; an
Internet-facing service, authentication adapters, rate limits, webhooks and an
OpenAPI description are not implemented yet.

## Trust boundary

- A merchant endpoint derives `tenant_id` and `merchant_id` from a verified
  merchant principal. It never trusts those values from a request body.
- A payment endpoint derives the payer account from an authenticated wallet
  session and verifies wallet authorization over the exact payment intent.
- A public payment-link read returns only presentation data required to pay.
  A checkout ID is not a bearer credential for merchant operations.
- Every request is bound to one configured network. Cross-network IDs and
  storage are rejected.
- Monetary values are decimal strings in atomic units. JSON floating-point
  numbers are forbidden.

Binary 32-byte opaque identifiers use unpadded base64url at the transport
boundary. User account fields use the canonical network-specific Bech32m
address. The eventual public schema must publish exact maximum lengths and
reject non-canonical encodings before domain conversion.

## Operations

The example paths are provisional and contain no release or version number.

### Create checkout

`POST /merchant/checkouts`

Requires an authenticated merchant principal. The request supplies:

- `order_key`: a caller-generated, cryptographically random 32-byte key unique
  within the merchant;
- `merchant_account`: the canonical settlement account;
- `asset_id` and fixed `amount`;
- `maximum_fee` and `fee_mode` (`customer_pays` or `merchant_sponsored`);
- `expires_at_ms` as an absolute UTC Unix timestamp;
- `valid_until_height`, derived by the service from verified finalized state
  and configured checkout policy rather than accepted blindly from a client.

The response contains `checkout_id`, the immutable definition, creation time
and `open` status. Repeating the same order key and exact definition returns the
existing checkout. Reusing the key with any changed field returns
`definition_conflict`; it never edits the prior request.

### Read public payment link

`GET /payment-links/{checkout_id}`

Returns the network, merchant display reference, settlement asset, amount,
maximum fee, fee mode, expiry and public state. It must not expose tenant IDs,
internal order keys, payer identity, risk decisions or gateway diagnostics.
Presentation metadata is stored outside the monetary definition and must not
change what the wallet signs.

### Pay checkout

`POST /payment-links/{checkout_id}/payments`

Requires an authenticated wallet principal. The payer is derived from that
principal. The body selects a fee not exceeding `maximum_fee`; it cannot change
the recipient, asset or amount.

The first request atomically binds the checkout to the payer and fee. An exact
retry returns the same payment identity and current state. A different payer or
fee returns `claim_conflict`, including after expiry. A new claim at or after
expiry returns `expired`.

The response must include both `checkout_status` and an explicit finality
indicator. Transport acknowledgement or gateway publication must never be
labelled as completed payment.

### Read merchant checkout status

`GET /merchant/checkouts/{checkout_id}`

Requires the owning tenant/merchant principal. The state is one of:

- `open`: unclaimed and not expired;
- `expired`: unclaimed and no longer payable;
- `payment_expired`: the claimed operation finalized after its signed height
  limit; it consumed the payer nonce but moved no amount or fee;
- `payment_starting`: claimed but not yet visible in the gateway journal;
- `processing`: reserved, nonce-assigned or signed but not published;
- `outcome_unknown`: publication outcome requires reconciliation;
- `accepted`: exact operation published, not final;
- `finalized`: a verified finalized receipt with `Applied` outcome exists;
- `rejected`: gateway policy or terminal payment rejection.

Merchant fulfilment is permitted only for `finalized`, unless a separately
approved risk policy explicitly accepts reversible exposure.

## Error contract

Transport adapters map typed domain errors without leaking storage or signer
details. At minimum they distinguish malformed input, unauthenticated,
unauthorized/not-owned, not found, expired, definition conflict, claim conflict,
fee policy violation, temporary unavailable and internal integrity failure.

Tenant mismatch is returned as not found to avoid cross-tenant enumeration.
Integrity, network-binding and impossible-state errors fail closed and emit a
protected operational alert. They are never converted into an empty checkout or
automatic repair.

## Retry and recovery rules

- Create idempotency is the merchant order key plus immutable definition.
- Payment idempotency is derived internally from the checkout ID and claimed
  payer; clients cannot select a second ledger idempotency key.
- Client timeout never authorizes a replacement payment.
- Pending dispatch survives restart and is retried with the exact gateway
  request.
- `accepted` remains non-final until reconciliation finds the finalized receipt.
- A finalized `Expired` receipt becomes `payment_expired`, never `finalized`.
- No automatic refund is inferred from a timeout or delayed finality.

## Required work before public exposure

- derive checkout `valid_until_height` server-side from verified finalized
  state and the requested wall-clock expiry; pool admission already enforces the
  configured maximum horizon, asset fee floor and expired-cleanup window;
- implement merchant and wallet authentication/authorization adapters;
- add request-size, rate, abuse and tenant-quota controls;
- add a fenced, backoff-aware dispatch service and operational metrics;
- define signed webhook events with sequence numbers, durable outbox,
  idempotent delivery and replay protection;
- add an OpenAPI/gRPC schema with canonical encoding test vectors;
- implement auditable cancellation-before-claim and refund-after-finality flows;
- run multi-validator latency, failure and duplicate-request tests against the
  checkout SLO.
