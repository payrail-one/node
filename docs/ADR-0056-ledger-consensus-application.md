# ADR-0056: Finality-gated ledger consensus application

## Status

Accepted for the BFT integration path.

## Context

The framework-neutral consensus boundary requires every validator to build or
re-execute the same signed-operation block and to publish authoritative state
only after independently verifying engine finality. The first Malachite
lifecycle used a test application that counted callbacks. It proved actor
wiring, signer journaling and WAL behavior, but it did not connect consensus to
the real payment runtime or LMDB.

Directly teaching a consensus vendor adapter about ledger rows or LMDB would
couple monetary correctness to one engine. Letting the engine callback write a
cursor without re-execution would permit a valid certificate for a value whose
payload does not reproduce the advertised block and state commitments.

## Decision

Introduce `ledger-consensus-application` as the engine-neutral application
adapter between BFT finality and the existing ledger/runtime/storage stack.

- `ConsensusPayloadSource` supplies a candidate canonical payload and may turn
  it into a compact consensus representation. A receiver must resolve the full
  payload before voting; missing data is rejected and resolved bytes cannot
  bypass runtime execution. See ADR-0058.
- `LedgerBlockExecutor` executes local proposals and independently re-executes
  remote or ordered values with real signature, nonce, replay, balance, fee and
  monetary invariant checks.
- `FinalizedLedgerStore` reads a normalized snapshot paired with its exact
  finalized cursor and exposes one commit operation for a fully verified tail
  block. `LmdbTailStateStore` implements this port through its existing atomic
  rows/tree/state/block/payload/cursor transaction.
- `ConsensusValueFinalityVerifier` verifies engine evidence against the exact
  network, epoch, parent, proposed commitments and payload identity. The
  Malachite verifier implements this port without exposing Malachite types to
  the ledger application.
- Network, membership epoch, validator-set hash and state-commitment policy are
  checked before execution or publication. Validator rotation remains disabled
  until a separately verified transition path exists.
- Proposal construction or validation retains a bounded, side-effect-free
  prepared transition keyed by the exact consensus value identity, parent and
  proposed checkpoint, together with the exact resolved full payload. No state
  is published from this cache.
- After evidence verification, the adapter passes the exact value through
  `TailSyncSession`. A private exact-proof capability prevents the already
  verified evidence from being replaced between engine verification and tail
  verification. An exact cached transition can then be reused; a cache miss is
  re-executed only after finality succeeds. Both paths compare the block and
  state commitments before producing the opaque verified block accepted by
  storage.
- A commit returns success only after the authoritative store has atomically
  published the normalized mutations, authenticated state, canonical state
  image, finalized block record, complete payload and finalized cursor.

## Failure behavior

- A malformed, unauthorized or commitment-mismatched peer value is rejected
  without changing storage.
- Local normalized-state corruption, signature worker failure or policy
  mismatch is a node error, not a peer penalty.
- Invalid or insufficient finality evidence is rejected before durable state
  changes.
- A stale parent or conflicting LMDB record fails closed. The application does
  not synthesize a cursor or retry against a different parent.
- Mempool removal is deliberately outside the authoritative commit. It is
  derived reconciliation and cannot make ledger publication partially fail.

## Verification

The integration tests initialize a signed-network-config-bound normalized LMDB
ledger, build a block containing a real Ed25519-signed transfer, validate it,
reject bad finality without mutation, commit valid evidence, close and reopen
LMDB, and verify balances, nonce, fee, finalized cursor and retained payload.
A forged state commitment is also rejected without advancing the store. A
compact-value test additionally proves that an unavailable payload cannot earn
an acceptance vote and that a receiving validator commits the reconstructed
full block rather than its manifest. A counting verifier proves that invalid
engine evidence adds no ledger signature verification and that local build,
validation and commit of the same exact value perform one authorization
verification in total. A cache miss still performs exactly one finality-gated
execution.

The three-validator Linux integration now runs this application on every node
over the authenticated permissioned QUIC/libp2p transport. A real
Ed25519-signed payment reaches Malachite quorum, then the complete committee is
stopped. The nodes reopen the same normalized LMDB environments, bounded WALs
and journal-first signer directories, retain their validator and transport
keys, advertise fresh network endpoints and finalize a second payment with the
next account nonce. All three LMDB environments reopen with the same two-block
checkpoint, balances, nonce, fees and retained payload history.

This is still not a production BFT network. Crash-at-each-durable-boundary,
same-endpoint process restart, partition healing, catch-up and injected durable
storage/signing failures remain open gates.
