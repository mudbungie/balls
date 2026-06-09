#!/usr/bin/env bash
# E2E driver for bl-1b49 — multi-agent parallel claims (worktree isolation).
#
#   scripts/e2e/bl-1b49-driver.sh          # the demo body (§0–§10 in the doc)
#   scripts/e2e/bl-1b49-driver.sh --race   # the concurrency-finding appendix
#
# Runs the FRESHLY-BUILT greenfield binary (bl + tracker + bl-delivery, first on
# PATH) against a throwaway /tmp repo under an isolated XDG_STATE_HOME, so it
# never touches this project's own task list. Every line of output is real.
set -uo pipefail

# Build location: this script lives in <repo>/scripts/e2e; binaries are built
# from the same checkout's target/release. Override REL to point elsewhere.
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REL="${REL:-$(cd "$HERE/../.." && pwd)/target/release}"
BIN=/tmp/bl-1b49-bin; RUN=/tmp/bl-1b49-run; PROJ=$RUN/acme
export XDG_STATE_HOME=$RUN/xdg
export GIT_AUTHOR_NAME=demo GIT_AUTHOR_EMAIL=demo@x
export GIT_COMMITTER_NAME=demo GIT_COMMITTER_EMAIL=demo@x

setup_bin() {
  rm -rf "$BIN"; mkdir -p "$BIN"
  ln -sf "$REL/bl" "$BIN/bl"
  ln -sf "$REL/tracker" "$BIN/tracker"
  ln -sf "$REL/bl-delivery" "$BIN/bl-delivery"
  export PATH="$BIN:$PATH"
}
say(){ printf '\n### %s\n' "$*"; }
run(){ printf '$ %s\n' "$*"; eval "$*" 2>/dev/null; }
runE(){ printf '$ %s\n' "$*"; eval "$*"; }

demo() {
  rm -rf "$RUN"; mkdir -p "$XDG_STATE_HOME" "$PROJ"; cd "$PROJ"

  say "0. FRESHLY-BUILT binary + throwaway cargo repo (isolated XDG_STATE_HOME)"
  run "command -v bl"
  git init -q -b main; cargo init -q --name acme --bin .
  # seed both module stubs so the crate compiles from commit 1; agents flesh out
  # DISJOINT files (engine.rs / api.rs) so drift re-merges with no shared edit.
  printf 'pub fn run(){}\n'   > src/engine.rs
  printf 'pub fn serve(){}\n' > src/api.rs
  printf 'mod engine;\nmod api;\nfn main(){ engine::run(); api::serve(); }\n' > src/main.rs
  git add -A && git commit -qm "Initial commit (cargo bin crate, engine+api stubs)"
  run "git -C '$PROJ' log --oneline"

  say "1. Found substrate + onboard worker alice (prime founds on first run)"
  runE "bl prime --as alice 2>&1 | grep -v '^{' || true"

  say "2. File four tasks at distinct priorities"
  run "T1=\$(bl create 'engine: core loop'  -p 1 -t backend  --as alice); echo \$T1"
  run "T2=\$(bl create 'api: http surface'  -p 2 -t backend  --as alice); echo \$T2"
  run "T3=\$(bl create 'cli: arg parsing'   -p 3 -t frontend --as alice); echo \$T3"
  run "T4=\$(bl create 'docs: user guide'   -p 4 -t docs     --as alice); echo \$T4"

  say "3. Ready-order priority — list returns highest-priority first"
  run "bl list -s ready"

  say "4. Two agents claim DISTINCT ready tasks; both hold claims at once"
  runE "bl claim '$T1' --as alice 2>&1 | grep -v '^{' || true"
  runE "bl claim '$T2' --as bob   2>&1 | grep -v '^{' || true"
  run "bl list -s claimed"

  say "5. No claim clobber — bob tries to take alice's already-claimed task"
  runE "bl claim '$T1' --as bob 2>&1 | grep -v '^{'; echo rc=\${PIPESTATUS[0]}"
  run "bl show '$T1' --json | python3 -c 'import sys,json;print(\"claimant still:\",json.load(sys.stdin)[\"claimant\"])'"

  say "6. Worktree isolation — two MIRRORED (non-%-encoded) paths, one per agent"
  run "git -C '$PROJ' worktree list | grep -E 'work/($T1|$T2)'"
  run "git -C '$PROJ' worktree list | grep -E 'work/($T1|$T2)' | grep -c '%' | sed 's/^/paths containing a percent sign: /'"

  say "7. Each worktree BUILDS independently under cargo/rust-lld (real link)"
  AW=$(git -C "$PROJ" worktree list | awk -v t="work/$T1" '$0 ~ t{print $1}')
  BW=$(git -C "$PROJ" worktree list | awk -v t="work/$T2" '$0 ~ t{print $1}')
  echo "alice worktree: $AW"; echo "bob   worktree: $BW"
  printf 'pub fn run(){ println!("engine up"); }\n' > "$AW/src/engine.rs"
  printf 'pub fn serve(){ println!("api up"); }\n'  > "$BW/src/api.rs"
  ( cd "$AW" && cargo build -q 2>&1 | tail -2; echo "alice cargo build rc=${PIPESTATUS[0]}" )
  ( cd "$BW" && cargo build -q 2>&1 | tail -2; echo "bob   cargo build rc=${PIPESTATUS[0]}" )
  git -C "$AW" add -A && git -C "$AW" commit -qm "engine: core loop"
  git -C "$BW" add -A && git -C "$BW" commit -qm "api: http surface"

  say "8. CONCURRENT-MAIN-DRIFT — alice closes first; main moves under bob's held claim"
  cd "$PROJ"
  runE "bl close '$T1' -m 'deliver engine' --as alice 2>&1 | grep -v '^{'; echo rc=\${PIPESTATUS[0]}"
  run "git -C '$PROJ' log --oneline main"
  echo "# bob's work/$T2 branched off the OLD main (1 commit); main is now ahead. Close must re-base bob's squash onto the drifted main:"
  runE "bl close '$T2' -m 'deliver api' --as bob 2>&1 | grep -v '^{'; echo rc=\${PIPESTATUS[0]}"

  say "9. Independent delivery, NO cross-contamination"
  run "git -C '$PROJ' log --oneline main"
  echo "# alice's commit touches engine.rs only, bob's touches api.rs only:"
  run "git -C '$PROJ' show --stat --format='%h %s' main~1 -- src/ | grep -E '\\.rs'"
  run "git -C '$PROJ' show --stat --format='%h %s' main    -- src/ | grep -E '\\.rs'"
  echo "# both deliveries coexist on main; main builds with BOTH merged:"
  run "git -C '$PROJ' grep -h 'println' main -- src/engine.rs src/api.rs"
  git -C "$PROJ" worktree add -q --detach "$RUN/verify" main
  ( cd "$RUN/verify" && cargo run -q 2>/dev/null; echo "main cargo run rc=${PIPESTATUS[0]}" )
  git -C "$PROJ" worktree remove --force "$RUN/verify"

  say "10. Final state — both tasks closed, worktrees torn down, history is the record"
  run "git -C '$PROJ' worktree list | grep -E 'work/' || echo '(no work/ worktrees remain)'"
  run "bl list -s ready"
  run "bl list -s closed"
  say "DONE"
}

# Appendix: fire two claims PHYSICALLY simultaneously against one local clone,
# N times, and classify the outcome (the bl-07d6 finding).
race() {
  local n=${1:-12} bothok=0 clean=0 wedged=0 i
  for i in $(seq 1 "$n"); do
    RUN=/tmp/bl-1b49-race/$i; PROJ=$RUN/acme; export XDG_STATE_HOME=$RUN/xdg
    rm -rf "$RUN"; mkdir -p "$XDG_STATE_HOME" "$PROJ"; cd "$PROJ"
    git init -q -b main; git commit -q --allow-empty -m init; bl prime --as alice 2>/dev/null
    local A B CLONE dirty nclaimed
    A=$(bl create 'a' -p 1 --as alice); B=$(bl create 'b' -p 2 --as alice)
    bl claim "$A" --as alice 2>/dev/null & bl claim "$B" --as bob 2>/dev/null & wait
    CLONE=$(echo "$XDG_STATE_HOME/balls/clones"/*/tasks)
    dirty=$(git -C "$CLONE" status --short 2>/dev/null | wc -l)
    if bl list -s ready >/dev/null 2>&1 && [ "$dirty" -eq 0 ]; then
      nclaimed=$(bl list -s claimed --json 2>/dev/null | python3 -c 'import sys,json;print(len(json.load(sys.stdin)))')
      if [ "$nclaimed" -eq 2 ]; then bothok=$((bothok+1)); else clean=$((clean+1)); fi
    else
      wedged=$((wedged+1))
    fi
  done
  printf '\n%s simultaneous-claim races: both_succeeded=%s one_lost_clean=%s clone_wedged_dirty=%s\n' \
    "$n" "$bothok" "$clean" "$wedged"
  echo "recovery for a wedged clone: git -C <clone>/tasks reset --hard  (see bl-07d6)"
}

setup_bin
case "${1:-}" in
  --race) race "${2:-12}" ;;
  *)      demo ;;
esac
