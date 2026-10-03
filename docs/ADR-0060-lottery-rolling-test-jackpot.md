# ADR-0060: Rolling test-credit jackpot for the lottery demo

## Status

Accepted for the Payrail testnet lottery demo. This is not a real-money prize
or a claim on a production asset.

## Context

The first lottery slice displayed a fixed `20,000x` top multiplier. It did not
persist an unclaimed top prize between rounds, split a prize among multiple
five-number winners, or expose the actual top-prize amount through the API.
That presentation looked like a jackpot without implementing one.

The demo already stores all monetary values as integer minor units and settles
entry prizes through D1 triggers. Jackpot accounting must preserve those
invariants, remain auditable with the committed draw, and be replay safe.

## Decision

1. A round starts with the previous drawn round's recorded rollover, or a
   configured `JACKPOT_SEED_MINOR` for the first round.
2. Every accepted entry adds its full test-credit price to both the round sales
   pool and the round jackpot. The D1 entry trigger performs this together with
   entry persistence and balance debit.
3. Payouts for two, three, and four matches remain fixed at `2x`, `25x`, and
   `500x`. Five matches receive an equal integer share of the round jackpot.
4. If nobody matches all five numbers, the whole jackpot rolls into the next
   round. After a win, the next round starts from the configured seed plus any
   indivisible remainder left after equal shares.
5. Settlement records winner count, total jackpot paid, and next rollover on
   the drawn round. Entry credits and the round transition execute in one D1
   batch; conditional updates make a repeated settlement harmless.
6. The public API returns jackpot fields as canonical decimal strings. The UI
   displays the live amount in `PR` while retaining explicit testnet and
   no-real-money labels.
7. The product-selected ticker is `PR` (PayRail). `PYL` is the documented
   fallback if a wallet, exchange, legal review, or market-data integration
   finds a material conflict with the shorter symbol.

## Consequences

The displayed jackpot is now derived from persisted accounting rather than a
hard-coded multiplier, and an unclaimed amount survives round rotation. The
seed is a demo guarantee funded outside ticket sales; a production launch would
require a licensed operator, segregated reserve accounting, jurisdictional
rules, limits, and an independently reviewed payout policy.
