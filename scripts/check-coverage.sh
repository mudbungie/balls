#!/usr/bin/env bash
# Run cargo-tarpaulin and fail if line coverage < 100%.
# Installs tarpaulin on first run if missing.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

if ! command -v cargo-tarpaulin >/dev/null 2>&1; then
    echo "cargo-tarpaulin not found. Install with: cargo install cargo-tarpaulin"
    exit 1
fi

THRESHOLD="${BALLS_COVERAGE_THRESHOLD:-100}"

# Tarpaulin's --fail-under flag exits non-zero when coverage is below the
# threshold.  --skip-clean keeps the incremental build fast enough for a hook.
echo "Running cargo tarpaulin (threshold ${THRESHOLD}%)..."
cargo tarpaulin \
    --engine llvm \
    --skip-clean \
    --fail-under "$THRESHOLD" \
    --out Stdout \
    --exclude-files 'target/*' \
    --timeout 120 \
    --color never \
    2>&1 | tail -40
