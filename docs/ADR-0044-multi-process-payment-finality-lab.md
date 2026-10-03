# ADR-0044: Multi-process signed-payment finality laboratory

## Status

Accepted for the Rust reference implementation and benchmark preparation;
extended to restart-persistent sequential blocks by ADR-0045 and a first-quorum
network fault profile by ADR-0048.

## Context

The existing laboratory proved two paths separately: seven mTLS validator
processes could produce a real weighted Ed25519 finality certificate, and
finalized signed-payment blocks could execute into the normalized LMDB ledger
atomically. Separate proofs do not continuously verify the complete path from a
candidate payment block through independent validator execution, guarded votes,
finality verification and durable publication.

A single synthetic execution TPS number is not evidence of payment finality.
Conversely, process startup, test-certificate creation and fixture generation
are not part of steady-state payment latency and must not contaminate the
measured interval.

## Decision

The initial `validator-process-harness` payment path established one bounded
laboratory round with 64 canonical Ed25519-signed transfers:

1. The parent creates a verified signed network configuration, canonical ledger
   snapshot, seven validator identities and one candidate payload.
2. Seven independent validator processes load the same verified configuration
   and state. Five are contacted in the default two-offline-node scenario.
3. The proposal travels as one fixed header and sequential bounded payload
   frames. The header commits to exact payload length, chunk count and a
   domain-separated SHA-256 digest. Missing, reordered, oversized, truncated or
   modified frames are rejected before runtime execution.
4. Every contacted validator canonically decodes and executes the signed block,
   recomputes its block hash and authenticated state root, and compares the
   complete proposed checkpoint.
5. Only after successful execution does the process durably reserve its
   precommit in `FileSigningJournal` and produce an Ed25519 vote. A journal or
   signer failure produces no vote.
6. The parent accepts votes only over mutually authenticated TLS with pinned
   certificate fingerprints, selects the first weighted quorum and verifies the
   resulting finality certificate.
7. The receiving path verifies finality before re-executing the transition.
   One LMDB transaction publishes normalized monetary rows, authenticated state,
   finalized block record, full payload and finalized cursor.
8. LMDB is closed and reopened. The recovered checkpoint, authenticated root,
   payload and operation sequence must match the finalized block exactly.

The report separates candidate execution, quorum collection, finality
verification plus durable commit, and the complete measured interval. Process
startup, identity/config generation, initial LMDB bootstrap and signed-payment
fixture generation happen before the timer.

## Security and operational boundaries

- This initial proof is a local-host, one-block laboratory round, not a
  production consensus implementation or published TPS result. ADR-0045 extends
  the executable command to a short sequential chain.
- The parent remains a central laboratory proposer. Production peer discovery,
  proposal propagation, view changes and fork choice are not modeled.
- Deterministic fixture keys are test-only and are deleted with the temporary
  workspace. They must never be reused outside the harness.
- The filesystem vote journal exercises reserve-before-sign semantics but is
  not an HSM or remote signer.
- A block hash commits to parent and canonical payload; independently verified
  deterministic execution binds that payload to the proposed state root before
  a vote is requested.
- The bounded chunk protocol is laboratory transport. The production P2P
  protocol must provide equivalent authenticated bounds and integrity.
- Multi-host latency, packet loss, sustained workloads, realistic account
  mixes, HSM latency, receipt-index load, soak tests and independent
  reproduction remain mandatory before any performance claim.

## Consequences

The repository now has an executable, restart-verified path joining real signed
payments, five-of-seven process quorum and the authoritative atomic LMDB commit.
It can detect regressions where validators sign a false state root, proposal
frames are corrupted, quorum is insufficient, finality is invalid or durable
state diverges after reopen. It does not close the production benchmark gate;
it supplies the first honest scenario that gate can extend across hosts and
long-running workloads.
