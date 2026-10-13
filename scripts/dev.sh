#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

export GULU_DATA_DIR="${GULU_DATA_DIR:-$PWD/dist/dev-data}"
export GULU_DIAGNOSTICS=1
exec cargo run --locked -p gulu-app "$@"
