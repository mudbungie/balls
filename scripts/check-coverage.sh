#!/usr/bin/env bash
# Run cargo-tarpaulin and fail if line coverage < 100%.
#
# THREE OUTCOMES, NOT TWO (bl-988d, ported from yog bl-673a). The exit code is
# what callers write a cached verdict from, and a stored FAIL is permanent:
# `speculate_run` stops the candidate chain at it on every later pass without
# rebuilding, and the key is the tree, so no re-run dislodges it.
#
#   exit 0   the tree covers 100% — a verdict; the hook records a PASS.
#   exit 75  EX_TEMPFAIL (`speculate_run::NO_VERDICT`, bl-1643): tarpaulin
#            reported being SIGNALED on both attempts — a kill from outside the
#            gate, not a judgement of the tree. `.github/workflows/speculate.yml`
#            and `bl-speculate run` record nothing on this code. Any caller that
#            only asks "did it pass?" (git's commit, `bl close`) still blocks.
#   other    the gate failed on the tree's own merits.
#
# THE RETRY IS ONCE, AND ONLY FOR THE SIGNALED CLASS: a real failure is never
# re-run. An interrupt to this script is no verdict either, so INT/TERM/HUP
# answer 75 — and are not retried (tarpaulin catches SIGINT and prints the very
# message matched below, so Ctrl-C would otherwise buy a second full run).

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

if ! command -v cargo-tarpaulin >/dev/null 2>&1; then
    echo "cargo-tarpaulin not found. Install with: cargo install cargo-tarpaulin"
    exit 1
fi

THRESHOLD="${BALLS_COVERAGE_THRESHOLD:-100}"
NO_VERDICT=75

log="$(mktemp)"
trap 'rm -f "$log"' EXIT
trap 'echo "coverage: interrupted — this run produced NO verdict." >&2; exit $NO_VERDICT' \
    INT TERM HUP

# Tarpaulin's own words for "I was signaled", as a FIXED string — never a
# variable: an empty pattern matches everything and would classify every real
# failure as infrastructure.
signaled() {
    grep -Fq 'Attempting to handle tarpaulin being signaled' "$log"
}

# Tarpaulin's --fail-under flag exits non-zero when coverage is below the
# threshold.  --skip-clean keeps the incremental build fast enough for a hook.
# The whole stream is held so the signaled line can be read; its tail is shown.
attempt() {
    cargo tarpaulin \
        --engine llvm \
        --skip-clean \
        --fail-under "$THRESHOLD" \
        --out Stdout \
        --exclude-files 'target/*' \
        --timeout 120 \
        --color never \
        >"$log" 2>&1
}

echo "Running cargo tarpaulin (threshold ${THRESHOLD}%)..."
code=0
for n in 1 2; do
    code=0
    attempt || code=$?
    tail -40 "$log"
    { [ "$code" -ne 0 ] && signaled; } || break
    echo "coverage: tarpaulin reports being SIGNALED (attempt $n) — a kill from" >&2
    echo "  outside the gate, not a verdict about this tree (bl-988d)." >&2
done

if [ "$code" -ne 0 ] && signaled; then
    echo "error: tarpaulin was signaled on both attempts — NO verdict for this" >&2
    echo "       tree; nothing may be cached from this run (exit $NO_VERDICT)." >&2
    exit "$NO_VERDICT"
fi
exit "$code"
