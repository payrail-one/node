#!/usr/bin/env bash
set -euo pipefail

readonly project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${project_root}"

bash scripts/check-source-size.sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
