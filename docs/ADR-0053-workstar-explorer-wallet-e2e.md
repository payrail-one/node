# ADR-0053: Workstar explorer, web wallet and real-transaction E2E

## Status

Accepted and implemented as a first single-node devnet vertical slice. The
prototype proves browser signing, runtime execution, atomic persistent ledger
publication and recovery of an independently derived receipt index. Production
application separation, a dedicated public explorer service, durable wallet
recovery and multi-validator finality remain open gates.

## Context

The platform needs a public explorer and a security-sensitive web wallet. They
must exercise the same signed transaction and finality path used by external
integrators. A UI that passes only mocked API tests does not prove that wallet
signing, submission, consensus, atomic ledger publication and indexing work
together.

Workstar is the selected TypeScript UI framework for the explorer, web wallet,
merchant and browser-admin surfaces. The reviewed repository is MIT licensed,
uses modular core/router/application packages, supports SSR/hydration and has no
mandatory vendor runtime. It is still an early framework, so versions must be
pinned exactly and compatibility must be verified before each upgrade.

## Decision

1. Use Workstar native components for production browser applications. Do not
   introduce React/Vue compatibility mode as the default architecture.
2. Keep two independently deployable applications:
   `apps/explorer-web` and `apps/wallet-web`. Share only focused packages such as
   the generated API client, canonical amount/address formatting, design tokens
   and transaction presentation. An integrated `apps/portal` is permitted for
   the first devnet acceptance slice only; it must not become a reason to couple
   wallet signing to explorer or validator internals.
3. The explorer consumes a rebuildable non-consensus index. It never reads or
   mutates validator LMDB directly and must label pending, finalized and failed
   states explicitly.
4. Production wallet signing is client-side or delegated to an explicitly
   authorized hardware/custody signer. Private material never reaches Workstar
   SSR, server actions, analytics, logs or explorer APIs.
5. JavaScript `number` is forbidden for amounts. APIs exchange canonical decimal
   strings; TypeScript clients parse them into checked `bigint` atomic units.
6. Pin exact Workstar, Playwright, TypeScript and build-tool versions, commit the
   lockfile and record license/SBOM evidence. The initial implementation must
   revalidate the then-current Workstar release instead of relying on `latest`.
7. Use strict CSP, trusted same-origin API endpoints, escaped rendering, safe URL
   policies, dependency review and reproducible static builds. Wallet pages must
   not load third-party scripts.
8. Every outgoing payment has a non-bypassable review step displaying exact
   recipient address, asset, amount, network and fee before browser signing.
   Human-facing forms accept checksummed network-bound addresses, not raw
   account identifiers.

## Implemented devnet evidence

The initial Workstar portal now generates a non-extractable browser Ed25519 key,
derives a Bech32m devnet address, validates the recipient network/checksum and
signs the canonical Rust-compatible transfer only after explicit confirmation.
Amounts remain bigint atomic units, the static container uses a same-origin API
proxy and strict security headers, and the UI labels its single-node devnet and
test asset honestly.

The Rust gateway executes both faucet and browser payments through the real
ledger block executor. Its authoritative LMDB commit atomically publishes the
normalized ledger mutation, authenticated root, finalized block, retained
payload and cursor. Receipts live in an independent rebuildable LMDB index; the
devnet explorer presentation is replayed from verified retained history after a
restart. Playwright creates two independent browser contexts, funds one,
submits a real signed transfer, verifies both balances and observes the same
finalized operation in the explorer without HTTP mocks. The Docker acceptance
run also restarts the gateway at height 2, verifies identical blocks and
transactions after recovery, and then advances the recovered chain to height 4.

## Docker E2E topology

The acceptance environment contains:

- an isolated ephemeral network/genesis and the real validator/node processes;
- public transaction/status APIs separated from validator administration;
- faucet holding test-only assets;
- finalized-event indexer and explorer read API;
- Workstar explorer and web-wallet static applications;
- a Playwright runner on the same isolated Docker network.

No production seed, key, endpoint or asset can enter this topology. Test wallet
keys are generated for one run, never committed and excluded from Playwright
traces, screenshots and console output.

## Required real-payment scenario

1. Wait for validator quorum, API, faucet and indexer readiness.
2. Create two ephemeral browser wallets on the test network.
3. Fund the sender through the real faucet transaction path and wait for indexed
   finality.
4. Build and sign a transfer in the browser over the canonical operation bytes.
5. Submit through the public API and capture the operation identifier.
6. Observe wallet state transition from submitted/pending to finalized without
   treating timeout as failure or finality.
7. Verify sender nonce, sender and recipient balances, fee, receipt and exact
   operation identifier from authoritative APIs.
8. Open the explorer and verify the same finalized block, operation, asset,
   accounts and amounts from the independent index.

The scenario may poll explicit status/finality conditions but may not use fixed
sleeps as correctness, directly write a database, bypass signatures or fulfill
network calls with mocks.

## Negative and recovery gates

- replaying the finalized envelope changes no balance and returns the canonical
  replay/idempotency result;
- a modified payload or signature is rejected and never appears as finalized;
- a wrong-network address cannot be submitted silently;
- loss of one non-quorum-critical node does not produce false finality;
- delayed indexer catch-up eventually reproduces the finalized receipt exactly;
- browser reload preserves only explicitly encrypted wallet state and never
  resubmits a transfer without user authorization;
- no secret appears in HTML, logs, source maps, screenshots, videos or traces.

## Implementation order

1. Freeze public transaction, status, block, receipt and asset schemas.
2. Implement the rebuildable indexer and explorer read API.
3. Generate the TypeScript API/transaction client from canonical schemas.
4. Scaffold the Workstar explorer and implement finality-aware navigation.
5. Implement client-side wallet creation/signing and secure local persistence.
6. Add the Docker topology and Playwright real-payment gate.
7. Add merchant flows only after the base transfer gate is reliable.

## Consequences

Workstar remains outside the monetary and consensus trust boundaries. The UI can
be replaced without changing ledger rules, transaction bytes or indexed event
semantics. Browser delivery waits on stable APIs, but API and indexer design must
now preserve the full E2E acceptance path from signed operation to explorer.
