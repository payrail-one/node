# ADR-0018: verified tail catch-up after snapshot recovery

## Status

Accepted as a runtime-neutral verification boundary and local integration path.
It is not a GRANDPA implementation or a production block executor.

## Context

A snapshot restores state at one finalized checkpoint but does not make a node
current. Before serving authoritative data or voting, the node must process every
subsequent finalized block without accepting gaps, forks, forged proofs or a
payload whose computed commitments differ from the finalized header.

## Decision

- `tail-sync-core` owns the runtime-neutral sequential catch-up invariants.
- A candidate must target exactly `current.height + 1` and name the current
  finalized block hash as its parent.
- Payload and finality proof sizes are bounded before verification.
- The configured `FinalityProofVerifier` must accept the complete checkpoint,
  including its validator-set hash.
- A side-effect-free `TailTransitionExecutor` prepares the next canonical state
  plus its block and state-root commitments. Both commitments must exactly match
  the finalized checkpoint.
- Verification returns an unforgeable `VerifiedTailBlock` but does not advance
  the cursor. The caller durably applies its payload first and calls `commit`
  afterwards. A stale verified block cannot advance an already changed cursor.
- The canonical codec rejects oversized, truncated, wrong-domain and trailing
  input.
- The multi-process recovery harness transfers three proof-bearing tail blocks
  after snapshot recovery and advances from height 100 to 103 through the shared
  domain session and strict Ed25519 finality verifier.

## Consequences and limits

The ordering/finality/commitment policy can be reused by a future runtime and
official GRANDPA adapter without importing harness code. The harness transition
executor is deterministic but does not yet execute real payment operations.
`tail-state-store-lmdb` atomically persists its prepared state, block record and
cursor; the production runtime must replace the opaque state image with its
normalized ledger mutations in that same transaction.
Authority-set changes remain the responsibility of the historical finality
provider and official SDK adapter.
