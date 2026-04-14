# balls — Agent Skill Guide

You are using **balls** (`bl`), a git-native task tracker. Tasks are JSON files in the repo. Worktrees isolate your work. Git provides sync.

## Roles

You may be acting as a **worker**, a **reviewer**, or both across different sessions on the same task. The commands and the rules differ — read the section that matches what the user asked you to do.

- **Worker**: `bl claim` → work in the worktree → `bl review`. Never close your own task.
- **Reviewer**: `bl show` / `git show` → `bl close` (approve) or `bl update status=in_progress` (reject). `bl close` must run from the repo root, **not** from inside the worktree.

If unsure: a worker is told to *do* something, a reviewer is told to *check*, *approve*, or *land* something.

## Session Start

Run `bl prime` at the start of every session:

```
bl prime --as YOUR_IDENTITY --json
```

It syncs with the remote and returns:
- **claimed**: tasks you already own — resume in their worktrees.
- **ready**: open tasks available to claim, sorted by priority.

Reviewers also want `bl list --status review` to see what's waiting on a decision.

## Commands

| Command | What it does |
|---------|-------------|
| `bl prime --as ID` [`--json`] | Sync, show ready + your claimed tasks. Run at session start. |
| `bl ready` [`--json`] | List open tasks ready to claim. |
| `bl list` [`--status STATUS`] | List non-closed tasks (use `--status review` to find reviewables). |
| `bl show TASK_ID` [`--json`] | Task details, including `delivered_in` sha after review. |
| `bl claim TASK_ID` | Worker: create worktree, set status=in_progress. |
| `bl review TASK_ID -m "msg"` | Worker: squash to main, set status=review. Worktree stays. |
| `bl close TASK_ID -m "msg"` | Reviewer: approve. Archive task, remove worktree + branch. **Repo root only.** |
| `bl update TASK_ID status=in_progress --note "..."` | Reviewer: reject. Bounces task back to the worker. |
| `bl update TASK_ID --note "text"` | Add a note (any role). |
| `bl drop TASK_ID` | Release a claim, remove worktree (worker self-recovery). |
| `bl dep tree` | Show dependency graph. |

## Task Lifecycle

```
open ──claim──> in_progress ──review──> review ──close──> archived
                     ^                    │
                     └────── reject ──────┘
```

- **open**: available to claim.
- **in_progress**: claimed; the worker owns it.
- **review**: worker submitted; waiting on a reviewer. Worktree still exists.
- **closed/archived**: task file deleted from the state branch's HEAD (not main). The work itself lives in main's git history.

A reject sets status back to `in_progress`. The worker resumes in their existing worktree; their next `bl review` re-merges main automatically.

## Worker Workflow

```
bl prime --as YOUR_ID
bl claim TASK_ID            # prints worktree path
cd <worktree path>          # work happens HERE, never in the main repo
# ... edit, commit ...
bl review TASK_ID -m "msg"  # submit; worktree stays for rework
```

Worktrees live at `.balls-worktrees/bl-xxxx`, on branch `work/TASK_ID`, with `.balls/local` symlinked for shared lock state.

Important:
- **Commit your work** before `bl review`. Review will `git add -A` anything left behind, but explicit commits give better history.
- **Don't modify files in the main repo** while working on a claimed task. Use the worktree.
- **Don't close your own task.** `bl review` is the worker's exit; the reviewer runs `bl close`.
- **Don't `bl update TASK_ID status=closed`** on a claimed task — it's rejected. Use `bl review`.
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

## Reviewer Workflow

A task in `review` status is waiting for a reviewer to approve or reject it. The work has already squashed to main; the reviewer's job is to accept it or send it back.

```
bl list --status review                       # find tasks awaiting review
bl show TASK_ID                               # status, claimant, delivered_in sha
git show <delivered_in sha>                   # inspect what landed on main
# (optional) peek at the worker's worktree state without editing:
#   ls .balls-worktrees/bl-xxxx
cd <repo root>                                # MUST be at repo root for close

# Approve:
bl close TASK_ID -m "approval note"

# Reject:
bl update TASK_ID status=in_progress --note "what to fix" --as reviewer
```

Important:
- **`bl close` must run from the repo root.** It removes the very worktree you'd be standing in; running from inside that worktree errors out with "cannot close from within the worktree".
- **Don't edit files inside the worker's worktree** to fix their work. Reject with notes and let the worker iterate. Their lock state and history stay clean.
- **Approval is durable.** Close archives the task file from the state branch HEAD and removes the worker's worktree + `work/TASK_ID` branch. The squash commit on main carries the work.
- **Rejection keeps the worktree.** The worker resumes in place; their next `bl review` re-merges main automatically.
- If the work needs to be undone entirely instead of accepted, that's a `git revert` of the delivered sha on main, not a `bl close` thing — close still archives the task.

## Creating Tasks

If you discover work that needs doing:

```
bl create "Fix the auth timeout" -p 1 -t bug --tag auth -d "Description here"
bl create "Add rate limiting" -p 2 --dep bl-a1b2 --tag api
```

- Priority: 1 (highest) to 4 (lowest)
- Types: `epic`, `task`, `bug`
- Dependencies: `--dep TASK_ID` (blocks the new task until dep is closed)
- Tags: `--tag NAME` (freeform labels)

## Dependencies and Links

```
bl dep add TASK_ID DEPENDS_ON_ID    # blocking dependency
bl dep rm TASK_ID DEPENDS_ON_ID     # remove dependency
bl dep tree                          # show full dependency graph
bl link add TASK_ID relates_to OTHER_ID   # non-blocking relationship
bl link add TASK_ID duplicates OTHER_ID
bl link add TASK_ID supersedes OTHER_ID
```

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
