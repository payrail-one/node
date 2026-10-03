# ADR-0045: Restart-persistent multi-block payment finality laboratory

## Status

Accepted for the Rust reference implementation and benchmark preparation;
extended with derived receipt-index outage recovery by ADR-0046 and a
first-quorum network fault profile by ADR-0048. ADR-0049 adds a separate bounded
persistent-process mode without replacing this restart-recovery scenario.

## Context

ADR-0044 joined independent signed-payment execution, five-of-seven finality and
an atomic normalized LMDB commit for one block. One successful block does not
prove that validators advance only after finality, survive process restart,
enforce the next parent/height/nonce or retain a complete sequential block
archive. Repeating isolated genesis rounds would hide those failures.

## Decision

The payment-finality harness now executes a short sequential chain with a
two-phase validator protocol:

1. Every validator loads a bounded canonical validator-set file and requires
   its network, epoch and content hash to match the signed network context.
2. A restarted validator opens its own LMDB, validates its current finalized
   checkpoint and independently executes the next bounded signed-payment block.
3. It durably reserves the exact `(set, round, stage, target)` before returning
   an Ed25519 precommit. Execution and a vote do not advance ledger state.
4. The coordinator constructs and strictly verifies a weighted certificate,
   then sends the bounded canonical proof to every validator that returned a
   valid vote over the same mutually authenticated TLS stream.
5. Each validator independently verifies finality and the sequential
   parent/height, atomically commits normalized monetary rows, authenticated
   state, block record, full payload and cursor to its LMDB, and only then sends
   a checkpoint-bound commit acknowledgement.
6. The coordinator requires a valid commit acknowledgement from every
   responder before committing the same prepared transition to its authoritative
   LMDB. The next round uses the recovered cursor and state, new sequential
   nonces and a new durable signing-journal slot.
7. Validator processes are stopped and recreated between blocks. The final
   audit reopens the coordinator store and verifies every retained parent,
   checkpoint and payload, the block count and the final operation sequence.

The default command executes four blocks of 64 real signed transfers. Process
startup, identity generation and genesis bootstrap remain outside the measured
interval. Aggregate timings are diagnostic local-lab evidence only.

## Failure semantics

- Insufficient quorum produces no certificate and no finality commit message.
- An invalid, malformed or oversized finality message cannot advance validator
  LMDB state or produce a commit acknowledgement.
- A responder that votes but fails to durably commit makes the laboratory round
  fail rather than silently reducing the durable replica count.
- A stale validator rejects the next parent. Catch-up is deliberately delegated
  to the separately tested authenticated state-sync path.

## Security and operational boundaries

- The coordinator remains a central laboratory proposer; this is not a P2P
  block-production, view-change or fork-choice implementation.
- The same two validators stay uncontacted in the default scenario. Rotating
  failures requires state sync before a previously offline validator can vote.
- Deterministic keys and the filesystem signing journal are test-only. HSM and
  remote-signer latency remain outside this proof.
- Four local blocks prove short-chain correctness and restart recovery, not
  sustained throughput, production latency, multi-host behavior or soak
  reliability.
- Production recovery must handle a coordinator failure after some validators
  commit by retrieving the finalized certificate/block and reconciling peers;
  the ephemeral laboratory intentionally fails closed instead.

## Consequences

The executable path now detects invalid finality mutation, non-sequential
parents, nonce discontinuity, lost validator state across restart, signing-slot
reuse, missing durable acknowledgements and incomplete finalized history. It
closes the short multi-block correctness gap while leaving the reproducible
multi-host performance gate open.
