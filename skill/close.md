# bl close — deliver the work and archive the task

    usage: bl close <id> [-m MSG] [--as ID] [--remote URL]

Delivers your work and retires the task in one move: **fold `main` in → run the
repo's pre-commit hook → squash `work/<id>` to `main`**, then archive the task
and tear down the worktree.

## Flags

- `-m MSG` — commit note for the store journal.
- `--as ID` — worker identity.
- `--remote URL` — per-op store remote (see `bl prime --skill`).

## Examples

    bl close bl-1a2b -m "shipped"

## Delivery is gated

Delivery first folds `main` into your work branch — so what gets checked is what
actually **lands**, even if `main` moved while you worked — then runs the repo's
`pre-commit` hook on the result and **aborts the close if it fails**: the task
stays claimed and the worktree stays up for the fix. A repo with no executable
`pre-commit` hook is ungated (close delivers unchecked). A merge conflict with
`main` also aborts the close cleanly (no half-merge is left behind); merge
`main` into the worktree by hand, resolve, and close again.

The delivery commit lands on `main` tagged `[bl-xxxx]` — that tag is how a merge
is recognized as the task's delivery.

Close does **not** push the code remote; pushing `main` is your own deliberate
`git push`.

## Closing IS the only retirement

A closed task has **no file** (absence = resolved); its history is the record.
To abandon a held task, `bl unclaim` then `bl close` — an empty worktree
delivers no code, so a `close`-gate guards every way a task can die. Closing a
task that still has live children prints a notice ("closed with N open children,
none gating") — informational, never a block; the children survive with
dangling, display-only parent pointers.

## Submit/approve flows

The default is solo: the agent that claims also closes. For a split flow, add a
review gate as an ordinary close-blocker subtask (`bl create "review X" --parent
X --blocks close`, or a forge plugin that mints one at claim). Submission is
git-native — push the work branch and open the PR yourself with the `[bl-id]`
tag in the PR title so the merge is recognized as the delivery.
