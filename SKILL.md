# balls — Agent Skill Guide

You are using **balls** (`bl`), a git-native task tracker. Tasks are JSON files in the repo. Worktrees isolate your work. Git provides sync.

## Core Workflow

```
bl prime --as YOUR_IDENTITY    # see what's ready and what you own
bl claim TASK_ID               # get a worktree, start working
# ... do your work in the worktree ...
bl review TASK_ID -m "summary" # submit for review (safe — worktree stays)
```

**Never run `bl close`.** Close is a reviewer/supervisor operation. Your job ends at `bl review`.

## Commands You'll Use

| Command | What it does | When to use it |
|---------|-------------|----------------|
| `bl prime --as ID` | Sync, show ready tasks + your claimed tasks | Session start |
| `bl prime --as ID --json` | Same, machine-readable | Session start (structured) |
| `bl claim TASK_ID` | Create worktree, set status=in_progress | Starting a task |
| `bl review TASK_ID -m "msg"` | Merge work to main, set status=review | Done with a task |
| `bl show TASK_ID` | View task details | Checking requirements |
| `bl show TASK_ID --json` | Same, machine-readable | Programmatic access |
| `bl ready` | List tasks ready to claim | Finding work |
| `bl ready --json` | Same, machine-readable | Finding work (structured) |
| `bl list` | List all non-closed tasks | Overview |
| `bl update TASK_ID --note "text"` | Add a note | Progress updates |
| `bl dep tree` | Show dependency graph | Understanding blockers |

## Task Lifecycle

```
open ──claim──> in_progress ──review──> review ──close──> archived
                     ^                    │
                     └────── reject ──────┘
```

- **open**: Available to claim.
- **in_progress**: You own it. Work in the worktree.
- **review**: You submitted. Worktree stays. Wait for approval.
- **closed/archived**: Task file deleted from HEAD. Work is in git history.

If a reviewer rejects (sets status back to `in_progress`), resume in your existing worktree. Your next `bl review` will merge main first.

## Working in a Worktree

When you `bl claim`, the output is the worktree path. **Change to that directory** to work:

```
/home/user/repo/.balls-worktrees/bl-a1b2
```

The worktree is a full checkout on a branch named `work/TASK_ID`. All your changes are isolated from main and other tasks.

Important:
- **Commit your work** before running `bl review`. Review will `git add -A` and commit anything uncommitted, but explicit commits give you better history.
- **Don't modify files in the main repo** while working in a worktree. Use the worktree.
- The worktree has `.balls/local` symlinked for shared lock/claim state.

## What `bl review` Does

1. Commits all uncommitted work in your worktree
2. Merges main into your worktree (catches up, surfaces conflicts HERE not on main)
3. Sets task status to `review`
4. Merges your branch into main with `--no-ff` (preserves branch topology)

If step 2 produces a merge conflict, review fails. Resolve the conflict in your worktree, then run `bl review` again.

## What NOT to Do

- **Don't run `bl close`** — that's for reviewers, and it rejects if you're in the worktree.
- **Don't run `bl update TASK_ID status=closed`** — on claimed tasks this is rejected. Use `bl review`.
- **Don't edit files on main directly** — use worktrees. Other workers may be closing tasks on main concurrently.
- **Don't delete `.balls/` files manually** — use `bl drop` to release a claim, `bl repair --fix` to clean up orphans.

## Session Start

Run `bl prime` at the start of every session:

```
bl prime --as agent-alpha --json
```

This syncs with the remote and returns:
- **claimed**: Tasks you already own (resume in existing worktree)
- **ready**: Tasks available to claim (sorted by priority)

If you have a claimed task, resume in its worktree. If not, claim the top ready task.

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
| Task claimed by someone else | Pick another from `bl ready` |
| Worktree in bad state | `bl drop TASK_ID --force` (loses uncommitted work) |
| Orphaned claims/worktrees | `bl repair --fix` |
| Lost context mid-task | `bl prime` shows your claimed tasks |
