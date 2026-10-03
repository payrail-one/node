# ADR-0054: One checkout across QR, embedded web and SMS

## Status

Accepted and implemented as a real single-node devnet vertical slice after the
initial wallet and explorer. Production enablement remains gated by merchant
authentication, multi-validator finality, country payment rules, a production
SMS provider and security review.

## Context

A merchant needs to present the same payment at a counter, inside a website and
to a customer using SMS. Implementing separate monetary flows for those channels
would create inconsistent amounts, duplicate-payment risk and receipts that
cannot be reconciled. A visually convincing demo must also exercise the real
signed ledger path rather than a mocked checkout animation.

EMVCo defines merchant-presented and consumer-presented QR modes and permits
account-based and URL-based payment processes. Country schemes can impose more
specific formats. A generic phone camera can open an HTTPS link, while a native
wallet can later parse an approved payment-specific QR profile.

## Decision

1. The authoritative object is an immutable, fixed-amount, single-use checkout
   from `merchant-checkout-core`. It binds network, asset, amount, recipient,
   merchant order, fee policy, expiry and payment idempotency.
2. QR, hosted web and SMS contain only a bounded checkout locator or approved
   encoded projection. They never become an alternative source of monetary
   fields and never authorize a payment by possession alone.
3. The first devnet QR encodes a short same-origin HTTPS checkout URL so a normal
   camera can open it. It is labelled as a platform profile, not as certified
   EMV QR. EMVCo MPM and local scheme profiles are separate adapters behind the
   same checkout contract.
4. The wallet fetches the checkout over an authenticated network-bound API,
   verifies the response, displays merchant, asset, amount, fee, network and
   expiry, then signs the exact canonical transfer only after explicit approval.
5. A hosted embed runs in an isolated iframe or reviewed Web Component. Parent
   communication is limited to typed lifecycle events such as `ready`,
   `cancelled`, `failed` and `finalized`; messages validate exact origin,
   checkout ID and schema. No private key or signed envelope is sent to merchant
   analytics or the parent page.
6. SMS uses the same checkout ID and may include a short link plus
   human-readable amount, asset and reference. A link click is not approval.
   Feature-phone payment remains a custodial/assisted flow with PIN or stronger
   challenge, replay protection, idempotency, SIM-swap controls, low limits and
   a finality notification.
7. The visual system follows high-quality mobile-payment principles: one primary
   action, prominent amount, restrained animation, explicit authorization
   progress, unambiguous result and a durable receipt. It does not copy Apple
   artwork, trademarks or product-specific layouts.

## Security and privacy requirements

- Checkout URLs contain no private key, bearer merchant credential, PII or
  mutable amount.
- Dynamic links use an allowlisted HTTPS origin; arbitrary redirect targets are
  rejected.
- QR rendering is local and dependency versions are pinned; no checkout payload
  is sent to a third-party QR service.
- Expired, already-claimed, wrong-network and tampered requests fail closed.
- The merchant fulfils an order only after independently verified finality.
- Embed CSP, iframe sandbox flags and `postMessage` origins are covered by tests.
- SMS content and provider logs minimize PII and never contain wallet secrets.

## Acceptance and demo gate

Playwright creates a merchant checkout, renders a scannable QR, opens the same
payment URL in an independent browser context, creates or unlocks a payer wallet,
signs the exact payment, waits for finality and verifies the same transaction in
merchant status, wallet receipt and explorer. A second fixture embeds the hosted
checkout on a plain merchant page and verifies the strict messaging contract.
SMS tests inject a provider-neutral inbound event and verify that duplicate or
replayed confirmation cannot create a second transfer.

Only after this scenario is stable may it be used for the product-owner demo
video. The video retains visible devnet/test-asset labels and shows real block,
transaction and receipt identifiers.

## Implemented devnet evidence

The Rust gateway now persists immutable fixed-amount checkouts in a separate
network-bound LMDB database. A checkout ID is also the signed transfer's exact
idempotency key. The server rejects a wrong network, asset, recipient, amount,
fee, validity height or checkout ID before ledger execution, atomically claims
one payer, and reports finality only after the real ledger transaction commits.
Checkout settlement is reconstructed from retained verified block payloads on
restart.

The Workstar merchant surface renders the same checkout as a local QR canvas,
an isolated same-origin hosted iframe and a provider-neutral SMS preview. The QR
contains only an HTTPS payment locator. The payer sees a restrained payment
sheet, creates an ephemeral devnet wallet, reviews the exact intent and signs in
the browser. The iframe sends only origin-checked, checkout-bound lifecycle
events; it never sends a key or signed envelope to its parent.

Playwright verifies merchant creation, QR rendering, embed locator and SMS
warning, opens the QR payment path in an independent browser context, funds the
payer, signs and finalizes the exact checkout, refreshes merchant status and
finds the amount in the explorer without mocked payment HTTP. Actual SMS
delivery remains a later provider-adapter task; the current UI deliberately
offers a safe preview/copy operation only.

## Approval-code extension

A BLIK-inspired short-code domain and persistence core is now implemented as
another locator and approval channel over the same checkout. It is not BLIK
compatibility and will use its own brand and protocol. The cryptographically
random one-time numeric code is short lived, stored only as a network-bound
HMAC digest and invalidated for further claims after one checkout binds it.
Entering it only locates the checkout; the wallet or participating bank
application must still show the exact merchant, amount, asset and fee and
obtain explicit authorization. Public HTTP exposure remains gated on durable
attempt/rate limits, authenticated issuer sessions and a non-enumerable payer
notification channel. Bank, mobile-money and acquirer integrations belong
behind certified issuer/acquirer APIs rather than inside consensus or the
merchant page. See `ADR-0055-bank-app-approval-code-core.md`.

## Consequences

Channel presentation can evolve independently while payment identity and money
semantics remain consistent. The first version requires network access for a
dynamic checkout; true offline value transfer is not implied. Production EMV or
country-scheme compatibility remains explicit adapter and certification work.
