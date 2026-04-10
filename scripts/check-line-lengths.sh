#!/usr/bin/env bash
# Fail if any Rust source file is >= 300 lines.
# Used by the pre-commit hook and callable directly for ad-hoc checks.

set -euo pipefail

MAX=300
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

violations=0
while IFS= read -r file; do
    lines=$(wc -l < "$file")
    if (( lines >= MAX )); then
        echo "LINE LIMIT: $file has $lines lines (max $((MAX - 1)))"
        violations=$((violations + 1))
    fi
done < <(
    find src tests -type f -name '*.rs' 2>/dev/null | sort
)

if (( violations > 0 )); then
    echo
    echo "$violations file(s) over the ${MAX}-line limit. Split them into smaller modules."
    exit 1
fi
