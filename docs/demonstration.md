# balls — demonstration proof

A captured end-to-end run of the core stories against the **shipped greenfield
binary** (`bl`, the two sibling plugins `tracker` + `bl-delivery`), after the
legacy→greenfield cutover. This is the *demonstration proof* deliverable: every
block below is real output from a throwaway repo, not a mock-up.

Reproduce it yourself in a fresh git repo (the only setup is `git init` + one
commit). The JSON op-log lines (`{"ts":…,"lvl":"info",…}`) go to **stderr** and
are elided here for readability — redirect `2>/dev/null` to suppress them, as the
commands below do for the read verbs.

The cast: a project with two tasks — a backend the frontend depends on.

---

## 0. Setup — an ordinary git repo

```console
$ git init -q -b main && git commit -qm "Initial commit"   # README.md as the seed file
$ command -v bl
/home/.../.local/bin/bl
```

No `bl init`, no `.balls/` directory, no daemon. State will live outside the repo
under XDG once we prime.

## 1. Found the substrate + onboard — `bl prime`

`prime` founds local state on first run (there is no separate `init`, §12), then
syncs. Idempotent: re-running converges to a no-op.

```console
$ bl prime --as alice
```

## 2. File two tasks; the form needs the backend — `bl create --needs`

`create` is the one verb that prints to **stdout** — the minted id, alone, so it
captures cleanly into a shell variable.

```console
$ BACKEND=$(bl create "Wire auth backend" -p 1 -t backend --as alice)
$ echo "$BACKEND"
bl-9f1b

$ FORM=$(bl create "Add login form" -p 2 -t frontend --needs "$BACKEND" --as alice)
$ echo "$FORM"
bl-9419
```

The form carries a *create-time* blocker on the backend. Only the reciprocal
`--blocks` edge (naming this task on another) is create-only; every other field —
title, body, parent, priority, tags, extras, and a task's own blockers — is
overwriteable later with `bl update` (e.g. `--needs`/`--no-needs`, the §10 in-band
unlink for a mis-wired or cyclic edge).

## 3. Status is derived, never stored — `bl list`

No task file holds a `status`. The backend is **ready**; the form is **blocked**
because its `claim`-blocker is unresolved.

```console
$ bl list
ready    bl-9f1b  Wire auth backend  p1
blocked  bl-9419  Add login form  p2
```

## 4. The bedrock projection — `bl show --json`

`--json` is the lossless contract: raw stored frontmatter, literal integer
timestamps, **no derived fields**.

```console
$ bl show bl-9419 --json
{
  "blockers": [
    {
      "id": "bl-9f1b",
      "on": "claim"
    }
  ],
  "claimant": null,
  "created": 1780896530,
  "id": "bl-9419",
  "parent": null,
  "priority": 2,
  "tags": [
    "frontend"
  ],
  "title": "Add login form",
  "updated": 1780896530
}
```

## 5. Core enforces blockers — claiming the blocked task is refused

```console
$ bl claim bl-9419 --as alice
bl claim: authoring the base change failed: claim: bl-9419 blocked by unresolved bl-9f1b
$ echo $?
1
```

The refusal names the blocker, and nothing was mutated.

## 6. Claim the ready backend — a `work/<id>` worktree appears

`claim` prints `claim <id>` to stderr; the worktree path is *not* echoed. Find it
with `git worktree list` — the `work/<id>` line.

```console
$ bl claim bl-9f1b --as alice          # prints "claim bl-9f1b" on stderr
$ git worktree list | grep work/bl-9f1b
/home/.../.local/state/balls/plugins/bl-delivery/tmp/.../bl-9f1b  d0a637d [work/bl-9f1b]
```

The project path is **mirrored** into the worktree dir, not percent-encoded (a
cargo build dir cannot carry `%` — it breaks `rust-lld` linking), so the leading
`/` is dropped and the path nests literally.

The task is now **claimed** (occupancy = the `claimant` field):

```console
$ bl show bl-9f1b --json | grep claimant
  "claimant": "alice",
```

## 7. Do the work *in the worktree*, commit on `work/<id>`

All edits go in the worktree, never on `main` directly — `bl close` delivers only
the worktree's diff.

```console
$ cd "$(git worktree list | awk '/work\/bl-9f1b/{print $1}')"
$ printf 'pub fn authenticate(u:&str)->bool{ !u.is_empty() }\n' > auth.rs
$ git add auth.rs && git commit -qm "auth backend: authenticate()"
$ git log --oneline
cbde171 auth backend: authenticate()
d0a637d Initial commit
```

## 8. Close — deliver + archive + tear down, in one move

```console
$ cd ~/acme            # back to the repo root
$ bl close bl-9f1b -m "deliver auth backend" --as alice    # prints "close bl-9f1b" on stderr
```

`main` now carries the delivery as one `[bl-xxxx]`-tagged commit, and the file is
on `main`:

```console
$ git log --oneline main
b7a8a23 Wire auth backend [bl-9f1b]
d0a637d Initial commit

$ git show main:auth.rs
pub fn authenticate(u:&str)->bool{ !u.is_empty() }

$ git worktree list | grep work/bl-9f1b || echo "(removed)"
(removed)
```

## 9. The dependency resolved — the form is now ready

Closing the backend resolved the form's blocker. No status was ever written;
readiness is recomputed.

```console
$ bl list
ready    bl-9419  Add login form  p2
```

## 10. Closed tasks reconstruct from history

A closed task is *gone* from the live set (absence = resolved), but `show` and
`list -s closed/--all` reconstruct it from the most recent commit whose tree still
held it — with its retirement derived from the deletion commit.

```console
$ bl list -s closed
closed   bl-9f1b  Wire auth backend  p1  @alice

$ bl show bl-9f1b
closed   bl-9f1b  Wire auth backend
  status   closed
  retired  2026-06-08T05:31:07Z
  created  2026-06-08T05:28:50Z
  updated  2026-06-08T05:29:03Z
  claimant alice
  priority 1
  tags     backend
```

## 11. Release and abandon — `unclaim`, then `close`

`unclaim` releases occupancy and the worktree, returning the task to **ready**.
Abandoning is the composite: `unclaim` then `close` — the empty deliverable
makes the delivery a no-op, so `main` is untouched.

```console
$ bl claim bl-9419 --as alice && bl unclaim bl-9419 --as alice
$ bl list
ready    bl-9419  Add login form  p2

$ bl close bl-9419 --as alice
$ bl list                       # live set now empty (exit 0)
$ git log --oneline main        # the empty close delivered nothing
b7a8a23 Wire auth backend [bl-9f1b]
d0a637d Initial commit
```

---

## What this proves

| Story | Verified |
|---|---|
| Found-on-first-run (no `init`) | `bl prime` §1 |
| Tasks are files on a store branch, ids are minted | `bl create` §2 |
| Status is derived (ready/blocked/closed), never stored | §3, §10, §11 |
| `--json` is the lossless bedrock projection | §4 |
| One blocker primitive, create-time edges, core-enforced | §5, §6, §2 |
| A claim materializes a `work/<id>` worktree off `main` | §7 |
| Work is isolated to the worktree | §8 |
| `close` = squash-deliver to `main` + archive + teardown, atomically | §9 |
| Delivery lands one `[bl-xxxx]`-tagged commit; `main` is a changelog | §9 |
| Resolving a dependency makes the dependent claimable | §10 |
| Closed tasks reconstruct from history | §11 |
| `unclaim` + empty `close` release without delivering | §12 |

Every core story from `docs/architecture.md` survives the cutover, end to end,
against the shipped binary.
