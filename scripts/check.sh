#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
python3 -m unittest discover -s scripts/tests -v
