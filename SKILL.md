# balls — Agent Skill Guide

You are using **balls** (`bl`), a git-native task tracker. Tasks are JSON files in the repo. Worktrees isolate your work. Git provides sync.

## The default flow: finish your own task

**One agent takes the task all the way through: claim → work → review → close → done.** That's the default. The `review` status is a checkpoint, not a stopping point. If nobody else is lined up to review your work, *you* do the close step yourself and the task is finished. Do not leave tasks sitting in `review` waiting for a reviewer who does not exist.

There is one inviolable rule: **never run `bl close` while you're standing inside the worktree it would delete.** `cd` back to the repo root first. The binary will refuse otherwise with `cannot close from within the worktree`.

Splitting the work across two agents — one submits, a different one approves — is an opt-in pattern, covered at the end of this guide. Don't reach for it unless the user has actually set up a reviewer. An agent that submits and then stops is an agent that didn't finish its job.

## The worktree is the unit of work

`bl` doesn't just track tasks — it tracks the code change a task delivers. Claim creates a worktree. Review squashes that worktree's branch back to main. Close tears the worktree down. The worktree isn't a convenience layer wrapped around your real workflow; while a task is claimed, it *is* your workflow.

So the rule reads as a consequence: while you hold a claim, edits go in the worktree, not on the operating branch. Editing main directly bypasses the lifecycle — the worktree never sees the change, `bl review`'s squash captures the wrong diff, and the delivery commit on main no longer reflects what the task shipped. The task can close perfectly cleanly while leaving drift behind it.

This binds the balls workflow, not your repo. Outside a claimed task, your tree is yours. But once `bl claim` has printed a path, that path is where the work goes.

## Session Start

Run `bl prime` at the start of every session:

```
bl prime --as YOUR_IDENTITY --json
```

It syncs with the remote and returns:
- **claimed**: tasks you already own — resume in their worktrees.
- **ready**: open tasks available to claim, sorted by priority.

If you resume and find one of your own tasks sitting in `review`, that's not a wait state — it means you stopped short last time. Run `bl close` from the repo root and finish it.

### First-time setup in a repo

`bl init` bootstraps a repo that has never used balls. It is not a no-op:

- If the repo has zero commits, it creates an initial commit so balls has something to anchor to.
- It creates a `balls/tasks` orphan state branch (non-stealth mode) where task JSON lives, separate from `main`'s history.
- It adds `.balls/config.json`, `.balls/plugins/.gitkeep`, and a `.gitignore` entry for runtime state, then commits them to the current branch (`balls: initialize`).

If you're scripting against a fresh repo, expect `bl init` to add one commit to whatever branch you're on. Make any pre-existing commit you want on main *before* running it.

**No-git mode:** `bl init --tasks-dir /path` also works outside git repos. All commands work; `bl claim` requires `--no-worktree`; `bl review`/`bl close` are status flips with no merge.

## Commands

| Command | What it does |
|---------|-------------|
| `bl prime --as ID` [`--json`] | Sync, show ready + your claimed tasks. Run at session start. |
| `bl ready` [`--json`] [`--limit N`] | List open tasks ready to claim. `--limit N` caps output; text mode adds a `... and K more` footer when truncated. |
| `bl list` [`--status STATUS`] | List non-closed tasks (use `--status review` to find reviewables). |
| `bl show TASK_ID` [`--json`] [`--verbose`] | Task details, including `delivered_in` sha after review. |
| `bl create "TITLE" [-d DESC] [-p 1..4] [-t TYPE] [--parent ID] [--dep ID] [--tag T]` | File a new task. Prints the new task id to stdout. See **Creating Tasks** below. |
| `bl claim TASK_ID` [`--no-worktree`] [`--sync`/`--no-sync`] | Start work: create worktree, set status=in_progress. `--no-worktree` skips worktree creation (required in no-git mode). `--sync` forces a remote round-trip on this claim (closes the offline-agent claim-race window); `--no-sync` skips one even if the repo's `require_remote_on_claim` is on. |
| `bl review TASK_ID -m "msg"` | Squash to main, set status=review. Worktree stays. The task id is auto-appended to the subject — do **not** include `[bl-xxxx]` in your `-m`. In no-git mode, status flip only. |
| `bl close TASK_ID -m "msg"` | Finish: archive task, remove worktree + branch. **Repo root only.** In no-git mode, archives file directly. |
| `bl update TASK_ID status=in_progress --note "..."` | Multi-agent reject path: bounces a submitted task back to in_progress. |
| `bl update TASK_ID status=closed --note "..."` | Archive an unclaimed task (dupes, stale, decided-against). Archival not deletion — see **Removing unwanted or duplicate tasks**. |
| `bl update TASK_ID --note "text"` | Add a note. |
| `bl drop TASK_ID` | Release a claim, remove worktree. |
| `bl dep tree` [`--json`] | Show parent/child tree with deps and gates as inline annotations. |

> **Note for agents:** the human-facing output of `bl list`, `bl ready`, `bl show`, and `bl dep tree` uses status glyphs and colors when stdout is a tty. Always prefer `--json` for parsing. If you must scrape human output, pass `--plain` (or set `NO_COLOR=1`) for stable, glyph-free, ASCII-only text — but the `--json` shape is the supported machine contract.

## Task Lifecycle

```
open ──claim──> in_progress ──review──> review ──close──> archived
                     ^                    │      │
                     └────── reject ──────┘      └── blocked while open `gates` links exist
```

- **open**: available to claim.
- **in_progress**: claimed; whoever holds it owns it.
- **review**: work has been squashed to main; the task is one `bl close` away from done. In the default solo flow, this is a transient state — you flip through it as you finish. Only in a multi-agent setup does it mean "waiting on someone else."
- **deferred**: explicitly set aside with intent to revisit. Narrow meaning — see **Deferred vs closed** below. Not a trash can.
- **closed/archived**: task file archived from the state branch's HEAD (not main). The work itself, or the decision not to do the work, lives in main's git history.

If a reviewer does exist and rejects, they set status back to `in_progress`. You resume in your existing worktree; your next `bl review` re-merges main automatically.

A `bl close` is additionally blocked if the task has any open `gates` links — see the link-types table below.

### Removing unwanted or duplicate tasks

If a task shouldn't exist — a duplicate of another open ball, a stale idea from a past exploration, something you decided against — **close it. Don't defer it, don't leave it in the queue.** Closing archives the task file from the state branch's HEAD; it is *archival, not destruction* (the state-branch history still has it, so closed tasks are recoverable via git). There is no separate `bl delete` and you don't need one.

For an unclaimed task, one command does the job:

```
bl update bl-xxxx status=closed --note "dup of bl-yyyy"
```

Duplicates also get a link so the relationship is discoverable in the archive:

```
bl link add bl-xxxx duplicates bl-yyyy
bl update bl-xxxx status=closed --note "dup of bl-yyyy"
```

For a task you already claimed and now regret: `bl drop` releases the claim, then `bl update ... status=closed` archives it.

### Deferred vs closed

Use `deferred` only for work you've decided not to do *now* with an intent to revisit later — waiting on an upstream, needs data you don't have yet, blocked on a decision outside the repo. If you aren't going to revisit it, close it. `deferred` is not a synonym for "I don't want to touch this" or "I'm not sure if this is real" — that's what `closed` is for. A growing `deferred` pile is a smell: those should be either promoted back to `open` or closed out.

## Workflow

The whole path, start to finish, done by one agent:

```
bl prime --as YOUR_ID
bl claim TASK_ID             # prints worktree path
cd <worktree path>           # all edits go here (see "The worktree is the unit of work")
# ... edit, commit ...
bl review TASK_ID -m "msg"   # squash to main, flip status to review
cd <repo root>               # required for close
bl close  TASK_ID -m "msg"   # archive task, remove worktree
```

`bl review` is the penultimate step, not the finish line. If you're not in a multi-agent setup with a configured reviewer, keep going and run `bl close` yourself.

Worktrees live at `.balls-worktrees/bl-xxxx`, on branch `work/TASK_ID`, with `.balls/local` symlinked for shared lock state.

Important:
- **Commit your work** before `bl review`. Review will `git add -A` anything left behind, but explicit commits give better history.
- **Edits live in the worktree** — see "The worktree is the unit of work" above for why this is load-bearing, not a convention.
- **Don't `bl update TASK_ID status=closed`** on a *claimed* task — it's rejected. Use `bl review` (which flips it to `review`), then `bl close` from the repo root. (For *unclaimed* tasks you want to remove — duplicates, stale ideas — `bl update status=closed` is the correct command; see **Removing unwanted or duplicate tasks** above.)
- **Don't hand-edit or rm files under `.balls/`** — that's the on-disk store, and direct edits corrupt the state branch. To release a claim use `bl drop`; to clean up orphans use `bl repair --fix`; to remove an unwanted task close it (see above). The restriction is about the store format, not the task lifecycle.

### What `bl review` does

1. Commits all uncommitted work in your worktree.
2. Merges main into your worktree (catches up; conflicts surface HERE, not on main).
3. Squash-merges your branch into main as a single feature commit.
4. Writes the delivery hint and flips status to `review` on the state branch.

If step 2 conflicts, review fails. Resolve in the worktree, then retry.

### Commit messages: 50/72 shape

`bl review -m` uses the first line as the commit title and everything after the first newline as the body. The delivery tag `[bl-xxxx]` is appended to the title automatically.

Pass a structured message so `git log --oneline` stays readable:

```
bl review bl-abcd -m "$(cat <<'EOF'
Short imperative title under ~50 chars

Body paragraph explaining the change in detail. Wrap at ~72.
Add more paragraphs as needed — everything after the blank line
is preserved as the commit body.
EOF
)"
```

A single-line `-m "fix foo"` becomes `fix foo [bl-abcd]` with no body. Don't stuff a multi-sentence summary into a single line; that produces an unreadable `git log --oneline`.

## Multi-agent: split submitter and reviewer (opt-in)

Only use this pattern if the user has actually asked for it or set up a separate reviewer agent. Otherwise the submitter should close their own task per the default flow above.

When the roles *are* split, the reviewer's job is to accept or send back work that's already squashed to main:

```
bl list --status review                       # find tasks awaiting review
bl show TASK_ID                               # status, claimant, delivered_in sha
git show <delivered_in sha>                   # inspect what landed on main
cd <repo root>                                # required for close — see hard rule above

# Approve:
bl close TASK_ID -m "approval note"

# Reject:
bl update TASK_ID status=in_progress --note "what to fix" --as reviewer
```

Important:
- **Don't edit files inside the submitter's worktree** to fix things — reject with notes and let them iterate.
- **Approval is durable.** Close archives the task file from the state branch HEAD and removes the submitter's worktree + `work/TASK_ID` branch. The squash commit on main carries the work.
- **Rejection keeps the worktree.** The submitter resumes in place; their next `bl review` re-merges main automatically.
- If accepted work needs to be undone, that's a `git revert` of the delivered sha on main, not a `bl close` thing — close still archives the task.

## Creating Tasks

If you discover work that needs doing:

```
bl create "Fix the auth timeout" -p 1 -t bug --tag auth -d "Description here"
bl create "Add rate limiting" -p 2 --dep bl-a1b2 --tag api
bl create "Spike: retry strategy" --parent bl-a1b2        # child of an epic
```

- Priority: 1 (highest) to 4 (lowest)
- Types: `epic`, `task`, `bug`. The type is a label, not a behavior switch — `epic` does not gate anything, does not auto-link children, and imposes no workflow of its own. Use it as a visual/organizational marker for container work and wire real hierarchy with `--parent`.
- Parent: `--parent TASK_ID` establishes a hierarchical parent (e.g. an epic). Does not block; use `--dep` for that.
- Dependencies: `--dep TASK_ID` (blocks the new task until dep is closed, repeatable)
- Tags: `--tag NAME` (freeform labels, repeatable)

`bl create` prints the new bare task id (`bl-xxxx`) to stdout and nothing else on success — capture it directly in a shell variable when scripting.

## Dependencies and Links

```
bl dep add TASK_ID DEPENDS_ON_ID    # blocking dependency
bl dep rm TASK_ID DEPENDS_ON_ID     # remove dependency
bl dep tree                          # show full dependency graph
bl link add TASK_ID relates_to OTHER_ID   # non-blocking relationship
bl link add TASK_ID duplicates OTHER_ID
bl link add TASK_ID supersedes OTHER_ID
bl link add TASK_ID replies_to  OTHER_ID  # thread reply
bl link add TASK_ID gates       OTHER_ID  # post-review blocker — see below
```

Link types:

| Type | Enforced? | Blocks | Use for |
|---|---|---|---|
| `relates_to` | no | nothing | cross-reference |
| `duplicates` | no | nothing | mark a dup |
| `supersedes` | no | nothing | "this replaces that" |
| `replies_to` | no | nothing | threaded discussion |
| `gates` | **yes** | **close of the source task** | post-review audits (security, docs, coverage) — parent can't archive until gate targets close |

`gates` is the thing to reach for when "this task is done but I still need someone to audit it" is a hard requirement, not a convention. A parent with an open gate link will refuse `bl close` until the gate child is itself closed. Use `bl link rm PARENT gates CHILD` if you need to drop a gate explicitly (it leaves a commit trail). `dep` blocks claim of the child; `gates` blocks close of the parent — they are intentionally different primitives. See the README "Gates: post-review blockers" section for the full pitch and worked example.

### When to claim a gate target

If a task is the target of a `gates` link — i.e. some other task has `gates <this>` pointing at it — **do not claim it until that source parent is at status `review` or later.** Gate targets are audits of a near-final artifact. If you start the audit while the parent is still `open` or `in_progress`, you are reviewing code that does not exist yet; the work gets thrown away and redone once the parent actually lands on main.

Before claiming anything from `bl ready`, run `bl show <id>` and look at the `links` list. If another task has `gates` pointing at this one, also `bl show` the source parent: claim only when its status is `review`, `closed`, or otherwise past construction. `bl ready` does not filter these out for you — the rule is yours to apply.

(Exception: occasionally a user asks you to audit a design or spec *before* implementation. That's the user routing you explicitly — not something to infer from the queue.)

## Scripting with bl

When driving `bl` from a script or another agent, treat the following as the machine contract. Everything else in this doc is for humans reading task state — the contract below is what a loop can rely on.

**Machine-parseable stdout.** These commands accept `--json` and print a single JSON document to stdout (stderr is diagnostics only). Prefer `--json` over parsing the table output. Shapes differ by command:

| Command | Top-level shape |
|---|---|
| `bl prime --json` | object: `{identity, claimed, claimed_status, ready}` — `claimed` and `ready` are arrays of task objects |
| `bl ready --json` | bare array of task objects |
| `bl list --json` | bare array of task objects |
| `bl show ID --json` | object: `{task, children, closed_children, completion, dep_blocked, delivered_in_resolved, delivered_in_hint_stale}` — the task itself is under `task` |
| `bl dep tree --json` | bare array of root nodes, each with nested `children` |

Don't assume a `ready` key on `bl ready --json` just because `bl prime --json` has one — `bl ready` is a bare array. Iterate the top level directly.

**Commands that print a single token on success.** Capture stdout directly:

| Command | Stdout on success |
|---|---|
| `bl create ...` | bare task id, e.g. `bl-3f2a` |
| `bl claim TASK_ID` | absolute worktree path (or `claimed ID (no worktree)` with `--no-worktree`) |
| `bl link add A TYPE B` | `A TYPE B` confirmation line |

`bl review`, `bl close`, `bl update`, `bl drop`, `bl dep add/rm`, `bl init` print human status lines; don't grep them. Check the exit code (`0` = ok, non-zero = error with a message on stderr).

**Building a tree in one pass.** `--parent` plus `--dep` compose cleanly:

```
EPIC=$(bl create "Migrate auth layer" -t epic -p 1)
A=$(bl create "Extract token store" --parent "$EPIC")
B=$(bl create "Swap middleware"       --parent "$EPIC" --dep "$A")
bl link add "$EPIC" gates "$(bl create "Audit: rollback plan reviewed" --parent "$EPIC")"
```

Parents are hierarchy; deps are claim-time blockers; gates are close-time blockers on the source task. They do not interact — pick each one on its own merits.

**Identity.** Set `BALLS_IDENTITY` in the environment once, or pass `--as` on every command that accepts it (`prime`, `claim`, `update`). Scripts should prefer the env var so a single export covers the whole session.

**Exit codes and errors.** Every `bl` subcommand exits non-zero on failure and writes `error: <message>` to stderr. No command writes partial output on failure, so a non-zero exit means the stdout token you were about to capture is not there — always check `$?` before using it.

## Environment

| Variable | Purpose | Default |
|----------|---------|---------|
| `BALLS_IDENTITY` | Your worker identity | `$USER`, then `"unknown"` |

Set `BALLS_IDENTITY` in your environment or use `--as` on commands that accept it.

## Error Recovery

| Situation | Solution |
|-----------|----------|
| Merge conflict on `bl review` | Resolve in worktree, run `bl review` again |
| `bl close` errors "cannot close from within the worktree" | `cd` to the repo root, retry |
| Task claimed by someone else | Pick another from `bl ready` |
| Worktree in bad state | `bl drop TASK_ID --force` (loses uncommitted work) |
| Orphaned claims/worktrees | `bl repair --fix` |
| Stale `state branch records close for bl-xxxx...` warnings on `bl sync` (pre-0.3.8 gate closes, or abandoned deliveries) | `bl repair --forget-half-push <id>` per id, or `bl repair --forget-all-half-pushes` to retract every current one |
| Lost context mid-task | `bl prime` shows your claimed tasks |
