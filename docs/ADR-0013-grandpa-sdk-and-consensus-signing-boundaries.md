# ADR-0013: GRANDPA SDK and consensus-signing boundaries

## Status

Accepted as an isolated challenger integration boundary. Polkadot SDK is no
longer the assumed production foundation and remains blocked on an approved,
pinned stable release and dependency-license review. See ADR-0052.

## Decisions

1. `grandpa-finality-boundary` carries the official `FRNK` engine identifier,
   network, GRANDPA set ID, canonical authority-set hash and bounded opaque SCALE
   justification bytes to a replaceable backend.
2. The boundary rejects wrong network, engine, set ID, authority hash, empty,
   oversized, truncated and trailing proof data before invoking SDK code.
3. Only an official Polkadot SDK backend may claim to verify GRANDPA signatures,
   ancestry and authority-set transitions. A mock backend is test
   infrastructure and never production finality. Other consensus engines use
   their own separately reviewed proof adapters.
4. `consensus-signer-core` separates vote intent, engine payload encoding,
   durable anti-double-sign journal and HSM/remote signer.
5. The journal performs atomic durable compare-and-set on
   `(network, set_id, round, prevote/precommit)`. It is reserved before the HSM
   call. An identical retry is allowed; another target fails closed.
6. Signer or encoder failure never releases a reservation. Consensus private
   keys and SDK payload construction remain outside the domain guard.

## Dependency gate

The crates.io metadata inspected on 2026-09-20 reports:

- `sp-consensus-grandpa 30.0.0`: Apache-2.0;
- `sc-consensus-grandpa 0.44.0`: GPL-3.0-or-later WITH
  Classpath-exception-2.0.

The second dependency is not added to the proprietary workspace until counsel
documents distribution obligations and the engineering review pins one coherent
Polkadot stable release. This gate applies even when Polkadot is used only as a
bake-off challenger. Polkadot SDK publishes coordinated stable releases and
recommends keeping component versions aligned:
[official Polkadot SDK repository](https://github.com/paritytech/polkadot-sdk).

The official GRANDPA primitive describes a justification as a commit plus the
ancestry headers connecting precommit targets to the finalized target:
[sp-consensus-grandpa documentation](https://paritytech.github.io/polkadot-sdk/master/sp_consensus_grandpa/index.html).

## Remaining production work

- legal approval and SBOM/license evidence for the selected stable release;
- official SCALE decode and `GrandpaJustification` verification backend;
- scheduled/forced authority transition and historical set tracking;
- production database compare-and-set journal with crash and failover tests;
- HSM vendor adapter, key ceremony, quorum policy and audit logging;
- equivocation evidence/reporting without exposing signing keys.
