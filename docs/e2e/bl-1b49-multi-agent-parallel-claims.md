# E2E demo bl-1b49 — multi-agent parallel claims (worktree isolation)

A captured live run for epic **bl-9369** (Milestone E2E demo & validation
sweep). This is the artifact for child **bl-1b49**: two agents claim distinct
ready tasks and carry them through independently, proving the documented
multi-agent guarantees hold in practice.

- **Binary:** freshly built from `main` (`cargo build --release`), the three
  siblings `bl` + `tracker` + `bl-delivery`, put first on `PATH`.
- **Repo:** a throwaway `/tmp/bl-1b49-run/acme` — `git init` + a one-commit
  cargo **bin crate** (so the worktree builds exercise a real `cargo`/`rust-lld`
  link, not just a parse).
- **Isolation:** `XDG_STATE_HOME=/tmp/bl-1b49-run/xdg`, so this run never touches
  the balls project's own task list (per the epic's standing warning).
- **Agents:** `--as alice` and `--as bob`. They are *logically concurrent* —
  both hold a claim at the same time — while each agent's own `bl` invocations
  are serialized, the way two real (slow) LLM agents actually interleave. The
  physically-simultaneous-invocation edge is characterized separately in the
  [Concurrency finding](#concurrency-finding--simultaneous-invocations-against-one-local-clone)
  appendix.

The JSON op-log (`{"ts":…,"lvl":"info",…}`) is written to **stderr**; it is
elided below (filtered with `grep -v '^{'`) so the terse stderr confirmations and
real stdout remain. Read verbs use `2>/dev/null` directly.

What bl-1b49 asks to verify, and where each lands below:

| Requirement | Section |
|---|---|
| ready-order priority (highest first) | §3 |
| two agents claim **distinct** ready tasks, both held at once | §4 |
| **no claim clobber** (a held task can't be re-claimed) | §5 |
| worktree isolation: **mirrored, non-%-encoded** paths, one per agent | §6 |
| those paths actually **build under cargo/rust-lld** | §7 |
| the **concurrent-main-drift** path (close re-bases onto a moved `main`) | §8 |
| **independent** close/delivery, **no cross-contamination** | §9 |
| teardown + history-is-the-record | §10 |

---

```console
### 0. FRESHLY-BUILT binary + throwaway cargo repo (isolated XDG_STATE_HOME)
$ command -v bl
/tmp/bl-1b49-bin/bl
$ git -C '/tmp/bl-1b49-run/acme' log --oneline
bd70622 Initial commit (cargo bin crate, engine+api stubs)

### 1. Found substrate + onboard worker alice (prime founds on first run)
$ bl prime --as alice 2>&1 | grep -v '^{' || true

### 2. File four tasks at distinct priorities
$ T1=$(bl create 'engine: core loop'  -p 1 -t backend  --as alice); echo $T1
bl-a27e
$ T2=$(bl create 'api: http surface'  -p 2 -t backend  --as alice); echo $T2
bl-469b
$ T3=$(bl create 'cli: arg parsing'   -p 3 -t frontend --as alice); echo $T3
bl-9d4b
$ T4=$(bl create 'docs: user guide'   -p 4 -t docs     --as alice); echo $T4
bl-9f2e

### 3. Ready-order priority — list returns highest-priority first
$ bl list -s ready
ready    bl-a27e  engine: core loop  p1
ready    bl-469b  api: http surface  p2
ready    bl-9d4b  cli: arg parsing  p3
ready    bl-9f2e  docs: user guide  p4

### 4. Two agents claim DISTINCT ready tasks; both hold claims at once
$ bl claim 'bl-a27e' --as alice 2>&1 | grep -v '^{' || true
claim bl-a27e
$ bl claim 'bl-469b' --as bob   2>&1 | grep -v '^{' || true
claim bl-469b
$ bl list -s claimed
claimed  bl-a27e  engine: core loop  p1  @alice
claimed  bl-469b  api: http surface  p2  @bob

### 5. No claim clobber — bob tries to take alice's already-claimed task
$ bl claim 'bl-a27e' --as bob 2>&1 | grep -v '^{'; echo rc=${PIPESTATUS[0]}
bl claim: authoring the base change failed: claim: bl-a27e is already claimed by alice
rc=1
$ bl show 'bl-a27e' --json | python3 -c 'import sys,json;print("claimant still:",json.load(sys.stdin)["claimant"])'
claimant still: alice

### 6. Worktree isolation — two MIRRORED (non-%-encoded) paths, one per agent
$ git -C '/tmp/bl-1b49-run/acme' worktree list | grep -E 'work/(bl-a27e|bl-469b)'
/tmp/bl-1b49-run/xdg/balls/plugins/bl-delivery/tmp/bl-1b49-run/acme/bl-469b bd70622 [work/bl-469b]
/tmp/bl-1b49-run/xdg/balls/plugins/bl-delivery/tmp/bl-1b49-run/acme/bl-a27e bd70622 [work/bl-a27e]
$ git -C '/tmp/bl-1b49-run/acme' worktree list | grep -E 'work/(bl-a27e|bl-469b)' | grep -c '%' | sed 's/^/paths containing a percent sign: /'
paths containing a percent sign: 0

### 7. Each worktree BUILDS independently under cargo/rust-lld (real link)
alice worktree: /tmp/bl-1b49-run/xdg/balls/plugins/bl-delivery/tmp/bl-1b49-run/acme/bl-a27e
bob   worktree: /tmp/bl-1b49-run/xdg/balls/plugins/bl-delivery/tmp/bl-1b49-run/acme/bl-469b
alice cargo build rc=0
bob   cargo build rc=0

### 8. CONCURRENT-MAIN-DRIFT — alice closes first; main moves under bob's held claim
$ bl close 'bl-a27e' -m 'deliver engine' --as alice 2>&1 | grep -v '^{'; echo rc=${PIPESTATUS[0]}
close bl-a27e
rc=0
$ git -C '/tmp/bl-1b49-run/acme' log --oneline main
ba549d9 engine: core loop [bl-a27e]
bd70622 Initial commit (cargo bin crate, engine+api stubs)
# bob's work/bl-469b branched off the OLD main (1 commit); main is now ahead. Close must re-base bob's squash onto the drifted main:
$ bl close 'bl-469b' -m 'deliver api' --as bob 2>&1 | grep -v '^{'; echo rc=${PIPESTATUS[0]}
close bl-469b
rc=0

### 9. Independent delivery, NO cross-contamination
$ git -C '/tmp/bl-1b49-run/acme' log --oneline main
2389c0a api: http surface [bl-469b]
ba549d9 engine: core loop [bl-a27e]
bd70622 Initial commit (cargo bin crate, engine+api stubs)
# alice's commit touches engine.rs only, bob's touches api.rs only:
$ git -C '/tmp/bl-1b49-run/acme' show --stat --format='%h %s' main~1 -- src/ | grep -E '\.rs'
 src/engine.rs | 2 +-
$ git -C '/tmp/bl-1b49-run/acme' show --stat --format='%h %s' main    -- src/ | grep -E '\.rs'
 src/api.rs | 2 +-
# both deliveries coexist on main; main builds with BOTH merged:
$ git -C '/tmp/bl-1b49-run/acme' grep -h 'println' main -- src/engine.rs src/api.rs
pub fn serve(){ println!("api up"); }
pub fn run(){ println!("engine up"); }
engine up
api up
main cargo run rc=0

### 10. Final state — both tasks closed, worktrees torn down, history is the record
$ git -C '/tmp/bl-1b49-run/acme' worktree list | grep -E 'work/' || echo '(no work/ worktrees remain)'
(no work/ worktrees remain)
$ bl list -s ready
ready    bl-9d4b  cli: arg parsing  p3
ready    bl-9f2e  docs: user guide  p4
$ bl list -s closed
closed   bl-a27e  engine: core loop  p1  @alice
closed   bl-469b  api: http surface  p2  @bob
```

---

## What this proves

| Story | Verified |
|---|---|
| Ready set is priority-ordered, highest first | §3 |
| Two agents claim **distinct** tasks; both `claimant`s coexist | §4 |
| A held task can't be re-claimed — refusal names the holder, `claimant` unchanged (**no clobber**) | §5 |
| Each claim gets its **own** `work/<id>` worktree at a **mirrored, `%`-free** path | §6 |
| Those paths link under real `cargo`/`rust-lld` — the `%`-encoding regression (bl-f3e4) stays fixed | §6, §7 |
| **Concurrent-main-drift:** alice's close advances `main`; bob's close re-bases his squash onto the moved `main` and succeeds | §8 |
| Each delivery lands one `[bl-xxxx]` commit touching **only its own files** — no cross-contamination | §9 |
| `main` builds and runs with **both** deliveries merged | §9 |
| Close tears the worktree down; closed tasks reconstruct from history | §10 |

Reproduce with the driver at `scripts/e2e/bl-1b49-driver.sh` (freshly-built
binary, throwaway `/tmp` repo, isolated `XDG_STATE_HOME` — every line above is
real output).

---

## Concurrency finding — simultaneous invocations against one local clone

The run above interleaves agents the way slow LLM agents really do: overlapping
claims, but no two `bl` processes touching the substrate in the same instant. To
probe the harder edge, the demo also fires two `bl claim` invocations
**physically simultaneously** against the *same* local clone (same project path →
same XDG clone, the on-box multi-agent deployment):

```console
$ bl claim "$A" --as alice 2>/dev/null &  bl claim "$B" --as bob 2>/dev/null &  wait
```

The two seals serialize on git's store-branch ref-lock — one wins, one loses:

```console
bl claim: ... cannot lock ref 'HEAD': is at <won> but expected <stale>   # OR
fatal: Not possible to fast-forward, aborting.
```

Across **12** such races: the loser **never** succeeded (there is no ref-lock
retry) — `both_succeeded=0  one_lost_clean=5  clone_wedged_dirty=7`.

The **no-clobber guarantee still holds** — the winner's claim is always intact,
the loser never overwrites it. But the losing path is not atomic. In 7/12 runs
the loser's `bl-delivery` pre-hook had already written `claimant` into
`tasks/<id>.md` in the clone working tree before the `--ff-only` seal aborted,
leaving that file modified **and staged**. That residue:

1. **wedges the clone** — every later op fails `Your local changes to
   tasks/<id>.md would be overwritten by merge`; and
2. **reads as a phantom claim** — `bl list`/`bl show` report the task claimed
   (from the dirty working tree) though it was never sealed, committed, or
   pushed and the loser holds no `work/<id>` worktree.

Recovery is `git -C <clone>/tasks reset --hard` (a plain `checkout -- .` does
**not** clear it — the write is staged); the winner's sealed claim survives.

Filed as **bl-07d6** (loser should roll the clone back atomically, and/or
bounded-retry the ref-lock loss — cf. the ETXTBSY busy-retry precedent). It does
not affect the logically-concurrent flow this demo's body proves.
