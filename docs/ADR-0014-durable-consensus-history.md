# ADR-0014: durable consensus signing and authority history

## Status

Accepted for the prototype boundary. This ADR does not approve a production
GRANDPA implementation or any particular HSM vendor.

## Context

A validator must not sign two targets for the same network, authority-set ID,
round and vote stage, including after a process restart. Historical finality
verification also needs the exact authority set active for an old proof. Both
properties must fail closed on corruption and must not depend on in-memory state.

## Decision

- `consensus-signer-journal-fs` stores one checksummed record per vote slot.
- A record is written and `fsync`ed under a unique temporary name, atomically
  published with a hard link, and followed by a directory `fsync`.
- Concurrent conflicting reservations race on one immutable target name; only
  one target can be published. Existing corrupt or non-regular records fail
  closed.
- `grandpa-authority-store-fs` persists checksummed, immutable authority-set
  records. Set IDs must start at zero and increase by exactly one; activation
  heights must increase; each record binds the previous canonical set hash.
- The authority store allows one writer through an exclusive ownership marker.
  A marker left by a crash is deliberately not auto-deleted: an operator must
  establish that no writer is alive before recovery.
- The store accepts only a transition the caller marks as already verified.
  Scheduled/forced-change proof verification remains the responsibility of the
  future official Polkadot SDK adapter.
- `HistoricalGrandpaFinalityAdapter` resolves the set ID carried by a bounded
  proof envelope and rechecks its hash before invoking the justification backend.

## Consequences

Restart and concurrent-reservation safety no longer depend on process memory,
and historical set selection is no longer a mutable "current set" lookup.
Filesystem durability still depends on the host filesystem honoring `fsync` and
hard-link atomicity. Production deployment must use a local filesystem with
documented semantics, monitored storage, backups, and a reviewed stale-lock
recovery runbook. Database/HSM integrations can implement the existing traits
without changing consensus-domain policy.
