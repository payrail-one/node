# ADR-0046: Derived receipt-index outage and recovery laboratory

## Status

Accepted for the Rust reference implementation and benchmark preparation;
the indexer is isolated as a supervised process by ADR-0047.

## Context

The normalized ledger deliberately excludes append-only receipts from live
consensus state. Finalized block payloads are retained by the authoritative
ledger store, while merchant, wallet and support lookups use a separate derived
receipt index. The index implementation and deterministic rebuild path were
already tested in isolation, but the multi-process finality laboratory did not
prove that ledger finality continues while the index is unavailable or that the
index can recover the exact process-finalized history afterward.

Making ledger commit depend on the derived index would widen the monetary
transaction boundary and let a query-system failure halt finality. Treating the
index as best-effort without verified catch-up would instead risk false payment
status and missing receipts.

## Decision

The restart-persistent payment-finality scenario now includes an independent
LMDB receipt-index workload:

1. After the first authoritative ledger block is finalized and reopened,
   `receipt-rebuild-core` reads the verified recovery base and retained block,
   re-executes its real signed operations and atomically indexes the receipts.
2. The index worker is then intentionally paused while the remaining payment
   blocks continue through validator execution, five-of-seven finality,
   validator LMDB commit acknowledgements and coordinator LMDB publication.
3. After the ledger chain completes, the receipt index is reopened. Rebuild
   starts from the same verified ledger base, verifies the already indexed
   block, replays every missing canonical payload and requires each computed
   block/state commitment to match the finalized archive.
4. Each missing receipt block is committed atomically with the receipt records,
   block record, operation-index/account-nonce/correlation secondary indexes and
   receipt cursor.
5. The final audit checks every operation index and cross-checks lookup by
   operation ID, account/nonce and signed correlation data. The receipt cursor
   must equal the authoritative ledger checkpoint and operation sequence.

The process report separates core finality timings from receipt-index work. The
index pause therefore cannot make the local consensus path appear slower or
hide that query availability lagged behind finalized funds.

## Failure semantics

- Receipt-index unavailability never rolls back or blocks a valid authoritative
  ledger commit.
- Wallet or merchant status may remain pending while the derived index is
  behind; it must not fabricate a final receipt.
- A gap, fork, corrupt payload, commitment mismatch, receipt mismatch or
  secondary-index conflict fails catch-up closed without partially advancing the
  receipt cursor.
- Replaying already indexed history verifies stored receipts rather than
  silently trusting or overwriting them.

## Security and operational boundaries

- The laboratory pauses a one-shot supervised process worker; long-running
  backoff, alerting and operational readiness probes remain future work.
- The short local chain proves deterministic recovery correctness, not sustained
  receipt-index capacity, online map growth, long retention or multi-host I/O
  performance.
- Receipt rebuild still performs full replay from the retained recovery base.
  Checkpointed bounded rebuild windows require a separate retention decision.

## Consequences

The joined laboratory now covers the complete durable path from signed payment
through process finality and authoritative LMDB publication to a restart-safe,
queryable receipt index. It demonstrates that derived-data failure is isolated
from monetary safety while making recovery completeness objectively testable.
