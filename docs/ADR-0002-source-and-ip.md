# ADR-0002: Public development node source and controlled production IP

Status: accepted for the public devnet

## Decision

The source required to build and inspect the Payrail development node may be
published in a public repository. Public network access, source visibility and
permission to reuse the source remain separate decisions.

The public repository is a development-network reference and must identify its
single-node finality, test-only assets and non-production security posture. It
must not contain production secrets, signing material, private infrastructure
addresses, personal operator details or deployment state.

Until a separate licensing decision is approved, the published Rust packages
remain `UNLICENSED` and non-publishable. Repository visibility alone grants no
license to copy, modify or redistribute the source.

Production validator implementation, operational tooling, risk services and
release artifacts remain subject to separate architecture, security, legal and
licensing review. Publishing the development node does not authorize opening
validator administration, consensus signing or production RPC boundaries.

## Public assurance

- protocol and transaction behaviour can be inspected against the running
  development network;
- builds use a committed lockfile and pinned toolchain version;
- releases should be signed and accompanied by hashes, SBOMs and build
  attestations before they are represented as production-ready;
- genesis, supply and finality evidence exposed by a public network must remain
  independently auditable;
- consensus and cryptography must remain secure when their behaviour is known.

## Publication gate

Before changing a repository to public visibility:

1. scan the complete public history for credentials and private infrastructure
   data;
2. publish from a clean, deliberately selected source snapshot;
3. verify formatting, tests and Clippy with warnings denied;
4. document how to run the node and its security limitations;
5. keep production keys and state outside source control and container images.

## Polkadot SDK licensing gate

No SDK dependency enters the production architecture without an inventory of its
exact version, license, modifications, linking model and distribution obligations.
FRAME crates may use permissive licenses while node/client crates can use GPL with
exceptions. Counsel must approve the assembled product, not merely the repository
headline license. If the obligations conflict with the proprietary-core goal, the
architecture bake-off must select a compatible component boundary or another
stack.
