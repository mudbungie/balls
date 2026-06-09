# E2E demo bl-7911 — greenfield bootstrap & solo claim→close lifecycle

A captured live run for epic **bl-9369** (Milestone E2E demo & validation sweep).
This is the artifact for child **bl-7911**: prove the documented greenfield
guarantees hold in practice, not just in unit tests.

- **Binary:** freshly built from `main` (`cargo build --release`), the three
  siblings `bl` + `tracker` + `bl-delivery`, put first on `PATH`.
- **Repo:** a throwaway `/tmp/bl7911-demo/acme` — `git init` + one empty commit,
  nothing more.
- **Isolation:** `XDG_STATE_HOME=/tmp/bl7911-demo/xdgstate`, so this run never
  touches the balls project's own task list (per the epic's standing warning).
- **Identity:** `--as solo` — one agent takes the task all the way through.

The JSON op-log (`{"ts":…,"lvl":"info",…}`) is written to **stderr**; it is
elided below (filtered with `grep -vE '^\{"ts":'`) so the terse stderr
confirmations and real stdout remain. Read verbs use `2>/dev/null` directly.

What bl-7911 asks to verify, and where each lands below:

| Requirement | Section |
|---|---|
| `bl prime` founds the substrate (no separate `init`) | §1b |
| §1 XDG state layout (state lives outside the repo) | §1c |
| read-time status derivation (ready → claimed → closed) | §3, §4c, §7d |
| worktree **materialize** on claim | §4, §4b |
| worktree **teardown** on close | §7c |
| delivery **squash to `main` tagged `[bl-xxxx]`** | §7a, §7b |

---

```console
===== 0. Setup — an ordinary git repo (no bl init, no .balls/) =====

$ git init -q -b main

$ git commit -q --allow-empty -m "Initial commit"

$ command -v bl
/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls/bl-7911/target/release/bl

$ ls -a
.
..
.git
```

No `bl init`, no `.balls/` directory, no daemon — just a git repo and the
freshly-built `bl` on `PATH`.

```console
===== 1. XDG state layout BEFORE prime — nothing exists yet =====

$ ls "$XDG_STATE_HOME/balls" 2>&1 || echo "(no balls state dir yet)"
ls: cannot access '/tmp/bl7911-demo/xdgstate/balls': No such file or directory
(no balls state dir yet)

===== 1b. Found the substrate — bl prime (no separate init, §12) =====

$ bl prime --as solo

===== 1c. §1 XDG state layout AFTER prime — landing + store live OUTSIDE the repo =====

$ find "$XDG_STATE_HOME/balls" -maxdepth 2 | sort
/tmp/bl7911-demo/xdgstate/balls
/tmp/bl7911-demo/xdgstate/balls/clones
/tmp/bl7911-demo/xdgstate/balls/clones/%2Ftmp%2Fbl7911-demo%2Facme

$ echo "repo still clean of bl artifacts:"; ls -a "$REPO"
repo still clean of bl artifacts:
.
..
.git
```

`prime` founded the substrate on first run — no `init` verb was ever needed. The
per-invocation-path clone dir is **percent-encoded** (`/tmp/bl7911-demo/acme` →
`%2Ftmp%2Fbl7911-demo%2Facme`) and lives under `$XDG_STATE_HOME/balls/clones/`,
entirely outside the project repo (which still holds only `.git`).

Inside that clone dir, the landing (`config/`, on `balls/config`) and the store
(`tasks/`, on `balls/tasks`) are two git worktrees of one repo:

```console
$ find "$XDG_STATE_HOME/balls/clones" -maxdepth 2 -mindepth 1 | sort
/tmp/bl7911-demo/xdgstate/balls/clones/%2Ftmp%2Fbl7911-demo%2Facme
/tmp/bl7911-demo/xdgstate/balls/clones/%2Ftmp%2Fbl7911-demo%2Facme/changes
/tmp/bl7911-demo/xdgstate/balls/clones/%2Ftmp%2Fbl7911-demo%2Facme/config
/tmp/bl7911-demo/xdgstate/balls/clones/%2Ftmp%2Fbl7911-demo%2Facme/log
/tmp/bl7911-demo/xdgstate/balls/clones/%2Ftmp%2Fbl7911-demo%2Facme/stealth.lock
/tmp/bl7911-demo/xdgstate/balls/clones/%2Ftmp%2Fbl7911-demo%2Facme/tasks

$ git -C .../config branch
* balls/config
+ balls/tasks

$ git -C .../tasks branch
+ balls/config
* balls/tasks
```

```console
===== 2. File a task — bl create prints the minted id to stdout =====

$ TASK=$(bl create "Add health endpoint" -p 1 -t backend --as solo 2>/dev/null); echo "minted: $TASK"
minted: bl-7b55
```

`create` is the only verb that prints to **stdout** — the minted id, alone, so it
captures cleanly into a shell variable.

```console
===== 3. Read-time status derivation — bl list (status is computed, never stored) =====

$ bl list 2>/dev/null
ready    bl-7b55  Add health endpoint  p1

===== 3b. The bedrock projection — bl show --json has NO status field =====

$ bl show bl-7b55 --json 2>/dev/null
{
  "blockers": [],
  "claimant": null,
  "created": 1780970706,
  "id": "bl-7b55",
  "parent": null,
  "priority": 1,
  "tags": [
    "backend"
  ],
  "title": "Add health endpoint",
  "updated": 1780970706
}
```

The task reads **ready** — unclaimed with no open blockers. The lossless `--json`
projection carries no `status` field at all: status is derived on read from
`claimant` + blocker resolution, never stored.

```console
===== 4. Claim — a work/<id> worktree materializes off main =====

$ bl claim bl-7b55 --as solo
claim bl-7b55

$ git worktree list
/tmp/bl7911-demo/acme                                                            283cd39 [main]
/tmp/bl7911-demo/xdgstate/balls/plugins/bl-delivery/tmp/bl7911-demo/acme/bl-7b55 283cd39 [work/bl-7b55]

===== 4b. §1 the code worktree lives under XDG (delivery plugin territory, path MIRRORED) =====

$ find "$XDG_STATE_HOME/balls/plugins" -maxdepth 4 -type d | sort
/tmp/bl7911-demo/xdgstate/balls/plugins
/tmp/bl7911-demo/xdgstate/balls/plugins/bl-delivery
/tmp/bl7911-demo/xdgstate/balls/plugins/bl-delivery/tmp
/tmp/bl7911-demo/xdgstate/balls/plugins/bl-delivery/tmp/bl7911-demo
/tmp/bl7911-demo/xdgstate/balls/plugins/bl-delivery/tmp/bl7911-demo/acme
```

`claim` **materialized** a worktree on branch `work/bl-7b55` off `main`. It lives
in the delivery plugin's territory, `…/balls/plugins/bl-delivery/<project-path>/<id>/`,
with the project path **mirrored** (not percent-encoded) — so it carries no `%`,
which would break `cargo`/`rust-lld` linking in that build dir.

```console
===== 4c. Status now derives to CLAIMED (claimant field set) =====

$ bl show bl-7b55 --json 2>/dev/null | grep claimant
  "claimant": "solo",

$ bl list 2>/dev/null
claimed  bl-7b55  Add health endpoint  p1  @solo
```

Occupancy is the `claimant` field; with it set, the very same task now reads
**claimed**. Nothing wrote the word "claimed" — it is recomputed.

```console
===== 5. Do the work IN the worktree, commit on work/<id> =====

$ WTPATH=$(git worktree list | awk '/work\/bl-7b55/{print $1}'); echo "worktree: $WTPATH"
worktree: /tmp/bl7911-demo/xdgstate/balls/plugins/bl-delivery/tmp/bl7911-demo/acme/bl-7b55

$ cd "$WTPATH" && printf 'pub fn healthy()->bool{true}\n' > health.rs \
      && git add health.rs && git commit -qm 'health endpoint' && git log --oneline
9afd0de health endpoint
283cd39 Initial commit
```

All edits go in the worktree, on `work/<id>`, never on `main` directly — `bl
close` delivers only the worktree's diff.

```console
===== 6. Close from the repo ROOT — deliver (squash to main) + archive + teardown =====

$ cd "$REPO" && bl close bl-7b55 -m 'deliver health endpoint' --as solo
close bl-7b55
```

One move: squash-deliver the worktree to `main`, archive the task, tear the
worktree down. Run from the repo root, never inside the worktree it deletes.

```console
===== 7a. main carries ONE [bl-xxxx]-tagged delivery commit =====

$ git log --oneline main
cd5649e Add health endpoint [bl-7b55]
283cd39 Initial commit

===== 7b. the worktree file is on main =====

$ git show main:health.rs
pub fn healthy()->bool{true}
```

The whole `work/bl-7b55` branch collapsed to a **single** commit on `main`,
titled with the task and tagged `[bl-7b55]` — `main` is the delivery changelog.
The worktree's file is now on `main`.

```console
===== 7c. worktree torn down (materialize/teardown verified) =====

$ git worktree list | grep 'work/bl-7b55' || echo '(worktree removed)'
(worktree removed)

$ find "$XDG_STATE_HOME/balls/plugins" -maxdepth 4 -type d | sort
/tmp/bl7911-demo/xdgstate/balls/plugins
/tmp/bl7911-demo/xdgstate/balls/plugins/bl-delivery
/tmp/bl7911-demo/xdgstate/balls/plugins/bl-delivery/tmp
/tmp/bl7911-demo/xdgstate/balls/plugins/bl-delivery/tmp/bl7911-demo
/tmp/bl7911-demo/xdgstate/balls/plugins/bl-delivery/tmp/bl7911-demo/acme
```

`close` removed the `work/bl-7b55` worktree — materialize (§4) and teardown are
both confirmed. (The mirrored parent path dirs remain as empty scaffolding; the
per-id worktree is gone.)

```console
===== 7d. read-time status derivation — closed task gone from live set, reconstructs from history =====

$ bl list 2>/dev/null; echo "(live set empty, exit $?)"
(live set empty, exit 0)

$ bl list -s closed 2>/dev/null
closed   bl-7b55  Add health endpoint  p1  @solo

$ bl show bl-7b55 2>/dev/null
closed   bl-7b55  Add health endpoint
  status   closed
  retired  2026-06-09T02:05:06Z
  created  2026-06-09T02:05:06Z
  updated  2026-06-09T02:05:06Z
  claimant solo
  priority 1
  tags     backend
```

The task file is **gone** from the store (absence = resolved), so the live set is
empty. But `list -s closed` / `show` reconstruct it from the most recent commit
whose tree still held it, deriving `retired` from the deletion commit. Status
across the whole run was never stored — only derived: **ready → claimed →
closed**.

---

## What this proves

| Guarantee (bl-7911) | Verified |
|---|---|
| `bl prime` founds the substrate on first run; no separate `init`, no `.balls/` in the repo | §1b, §1c |
| §1 XDG layout: percent-encoded clone holds `config/` (landing) + `tasks/` (store) outside the repo | §1c |
| §1 XDG layout: the `work/<id>` code worktree lives under `plugins/bl-delivery/<mirrored-path>/<id>` | §4b |
| Status is derived on read, never stored (ready → claimed → closed) | §3, §3b, §4c, §7d |
| `claim` **materializes** a `work/<id>` worktree off `main` | §4 |
| `close` **tears down** the worktree | §7c |
| `close` squash-delivers one commit to `main`, tagged `[bl-xxxx]` | §7a, §7b |
| A single agent runs claim → work → close to done (no review step) | §4–§7 |

Every greenfield guarantee in bl-7911 holds end to end against the freshly-built
binary in a throwaway repo.
