#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${GH_TOKEN:?GitHub token is required}"
: "${GH_REPO:?GitHub repository is required}"
: "${GITHUB_SHA:?Build commit is required}"
exec python3 scripts/publish-release.py "${1:-dist/release}"
