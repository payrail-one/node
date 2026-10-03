# ADR-0055: Bank-app approval-code core

## Status

Accepted for the reference devnet foundation. Issuer sessions, public API
exposure and production rollout remain gated.

## Context

BLIK demonstrates a valuable payment pattern: a customer obtains a short-lived
six-digit code from a trusted bank application, a merchant submits it through
its acquirer, and the customer confirms the exact payment in the bank
application. The code and the confirmation are distinct steps. The broader
distribution model—banks, acquirers and payment integrators participating in one
scheme—is more important than the visual six-digit code.

The platform needs a comparable first-party experience for bank, mobile-money,
wallet and white-label deployments. It must not claim BLIK compatibility or
treat a low-entropy numeric value as sufficient authorization.

## Decision

The approval-code rail is separate from ledger execution and reuses the same
immutable merchant checkout as QR, hosted web, SMS and USSD channels.

- The wallet or issuer creates a uniformly random value from the complete
  `000000`–`999999` space with the operating-system CSPRNG and rejection
  sampling, avoiding modulo bias.
- The code is valid for no more than 120 seconds and can be linked to exactly
  one payer account and one checkout.
- Persistence receives only an HMAC-SHA-256 digest bound to the network. The
  plaintext code is returned once to the issuer channel and is intentionally
  absent from stored records and debug output.
- LMDB atomically stores the record, active-account owner and checkout owner.
  Startup validates all forward and reverse indexes before serving requests.
- Issuing a new code rotates a previous unclaimed code. A live claimed code
  cannot be silently replaced. Expired, unknown, cross-checkout and cross-payer
  operations fail closed.
- Claiming only links the parties. Money moves only after the payer sees the
  merchant, amount, asset and fee and produces the normal signed checkout
  transaction. Finality then consumes the exact code/checkout/account binding.
- A consumed digest remains available for idempotent retry and audit until a
  later bounded retention policy permits safe reuse of the six-digit space.

The domain, HMAC authentication, secure random generation and LMDB persistence
are separate crates. This keeps scheme rules independent from HTTP, a bank SDK,
the wallet UI and the ledger database.

## Public API gate

The reference core is not yet exposed by the devnet gateway. A public endpoint
would also require all of the following in the same security increment:

- authenticated issuer/wallet sessions and explicit device binding;
- bounded attempts and durable per-device, account, tenant and network rate
  limits;
- an opaque authenticated channel for the payer to receive the claimed payment
  request without making account activity enumerable;
- merchant/acquirer identity, request signing and replay protection;
- risk scoring, step-up authentication, cancellation and safe recovery;
- low-cardinality monitoring and incident controls that never log plaintext
  codes.

Shipping a public six-digit lookup endpoint without those controls is rejected.

## Consequences

The platform now has a tested, restart-safe foundation for a BLIK-inspired UX,
while the actual payment remains protected by the existing signed transaction
and ledger invariants. Bank-wide and merchant-wide distribution is not a code
feature: it requires issuer/acquirer APIs, a certification kit, operational and
dispute rules, regulated partners and commercial agreements.
