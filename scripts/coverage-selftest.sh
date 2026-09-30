#!/bin/sh
# Drive the REAL `scripts/check-coverage.sh` under a fake `cargo` (bl-988d) —
# a step of the pre-commit chain, `make check` and ci.yml. Sub-second.
#
# The script's exit code is a verdict contract (0 pass, 75 no verdict, other
# fail), and speculate.yml writes a permanent FAIL from the third, so each case
# asserts the code AND how many tarpaulin runs it cost: a real failure retried
# is as wrong as a signaled one not retried.
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
COVERAGE="$here/check-coverage.sh"

fails=0
check() { # <0-if-true> <label>
    if [ "$1" -eq 0 ]; then printf 'ok   %s\n' "$2"; else printf 'FAIL %s\n' "$2"; fails=$((fails + 1)); fi
}

BOX=$(mktemp -d)
trap 'rm -rf "$BOX"' EXIT
# The fake cargo pops one outcome per run from $PLAN: pass, sig or fail.
cat > "$BOX/cargo" <<'SH'
#!/bin/sh
echo run >> "$RUNS"
step=$(head -n 1 "$PLAN"); tail -n +2 "$PLAN" > "$PLAN.n"; mv "$PLAN.n" "$PLAN"
case $step in
    pass) echo "100.00% coverage"; exit 0 ;;
    sig)  echo 'Error: "Failed to run tests: Attempting to handle tarpaulin being signaled"' >&2; exit 1 ;;
    *)    echo "test foo ... FAILED" >&2; exit 101 ;;
esac
SH
printf '#!/bin/sh\nexit 0\n' > "$BOX/cargo-tarpaulin"
chmod 755 "$BOX/cargo" "$BOX/cargo-tarpaulin"
PATH="$BOX:$PATH"
PLAN="$BOX/plan"
RUNS="$BOX/runs"
export PATH PLAN RUNS

case_() { # <plan words> <want code> <want runs> <label>
    printf '%s\n' $1 > "$PLAN"
    : > "$RUNS"
    "$COVERAGE" >/dev/null 2>&1 && rc=0 || rc=$?
    runs=$(wc -l < "$RUNS")
    check "$([ "$rc" -eq "$2" ] && [ "$runs" -eq "$3" ] && echo 0 || echo 1)" \
        "$4 (exit $rc after $runs run(s); want $2 after $3)"
}

case_ 'pass'        0   1 'a pass passes, once'
case_ 'sig pass'    0   2 'signaled once, then pass: retried and passes'
case_ 'sig sig'     75  2 'signaled twice: no verdict (75)'
case_ 'fail'        101 1 'a real failure keeps its code and is not retried'
case_ 'sig fail'    101 2 'signaled, then a real failure: a verdict (fail)'

if [ "$fails" -ne 0 ]; then
    printf '\n%s coverage selftest assertion(s) failed\n' "$fails" >&2
    exit 1
fi
printf '\ncoverage selftest: all assertions passed\n'
