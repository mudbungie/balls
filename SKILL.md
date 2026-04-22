# balls — Agent Skill Guide

You are using **balls** (`bl`), a git-native task tracker. Tasks are JSON files in the repo. Worktrees isolate your work. Git provides sync.

## The default flow: finish your own task

**One agent takes the task all the way through: claim → work → review → close → done.** That's the default. The `review` status is a checkpoint, not a stopping point. If nobody else is lined up to review your work, *you* do the close step yourself and the task is finished. Do not leave tasks sitting in `review` waiting for a reviewer who does not exist.

There is one inviolable rule: **never run `bl close` while you're standing inside the worktree it would delete.** `cd` back to the repo root first. The binary will refuse otherwise with `cannot close from within the worktree`.

Splitting the work across two agents — one submits, a different one approves — is an opt-in pattern, covered at the end of this guide. Don't reach for it unless the user has actually set up a reviewer. An agent that submits and then stops is an agent that didn't finish its job.

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
| `bl claim TASK_ID` [`--no-worktree`] | Start work: create worktree, set status=in_progress. `--no-worktree` skips worktree creation (required in no-git mode). |
| `bl review TASK_ID -m "msg"` | Squash to main, set status=review. Worktree stays. In no-git mode, status flip only. |
| `bl close TASK_ID -m "msg"` | Finish: archive task, remove worktree + branch. **Repo root only.** In no-git mode, archives file directly. |
| `bl update TASK_ID status=in_progress --note "..."` | Multi-agent reject path: bounces a submitted task back to in_progress. |
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
- **closed/archived**: task file deleted from the state branch's HEAD (not main). The work itself lives in main's git history.

If a reviewer does exist and rejects, they set status back to `in_progress`. You resume in your existing worktree; your next `bl review` re-merges main automatically.

A `bl close` is additionally blocked if the task has any open `gates` links — see the link-types table below.

## Workflow

The whole path, start to finish, done by one agent:

```
bl prime --as YOUR_ID
bl claim TASK_ID             # prints worktree path
cd <worktree path>           # work happens HERE, never in the main repo
# ... edit, commit ...
bl review TASK_ID -m "msg"   # squash to main, flip status to review
cd <repo root>               # required for close
bl close  TASK_ID -m "msg"   # archive task, remove worktree
```

`bl review` is the penultimate step, not the finish line. If you're not in a multi-agent setup with a configured reviewer, keep going and run `bl close` yourself.

Worktrees live at `.balls-worktrees/bl-xxxx`, on branch `work/TASK_ID`, with `.balls/local` symlinked for shared lock state.

Important:
- **Commit your work** before `bl review`. Review will `git add -A` anything left behind, but explicit commits give better history.
- **Don't modify files in the main repo** while working on a claimed task. Use the worktree.
- **Don't `bl update TASK_ID status=closed`** on a claimed task — it's rejected. Use `bl review` (which flips it to `review`), then `bl close` from the repo root.
- **Don't delete `.balls/` files manually.** Use `bl drop` to release a claim, `bl repair --fix` to clean up orphans.

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

## Scripting with bl

When driving `bl` from a script or another agent, treat the following as the machine contract. Everything else in this doc is for humans reading task state — the contract below is what a loop can rely on.

**Machine-parseable stdout.** These commands accept `--json` and print a single JSON document to stdout (stderr is diagnostics only): `bl prime`, `bl ready`, `bl list`, `bl show`. Prefer `--json` over parsing the table output.

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
| Lost context mid-task | `bl prime` shows your claimed tasks |
