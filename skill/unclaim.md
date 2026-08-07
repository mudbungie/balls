# bl unclaim — release a claim

    usage: bl unclaim <id> [--as ID] [--remote URL]

Releases your claim on a task and removes its `work/<id>` worktree.

## Flags

- `--as ID` — worker identity.
- `--remote URL` — per-op store remote (see `bl prime --skill`).
- `-C PATH` — **global** (every command): address the store keyed by PATH, as
  if `bl` had run there. No walking, no git-root discovery.

## Examples

    bl unclaim bl-1a2b

## What survives

Removing the worktree discards **uncommitted** work — it dies with the worktree.
Work you **committed** on the `work/<id>` branch **survives on this machine**: a
later `bl claim` + `bl close` here delivers it. To discard that committed work
too, delete the branch explicitly: `git branch -D work/<id>`.

Unclaim is the one teardown that keeps the branch, and that is the point — the
ball is going back on the board and nothing was delivered. `bl close` deletes it,
because by then the work is squashed onto the target.

The `work/<id>` branch is machine-local — the store syncs through the remote,
the work branch never does. A takeover from **another clone** materializes a
fresh, empty branch; the original WIP stays stranded where it was committed
until that machine itself claims + closes (or pushes the branch by hand).

There is no separate `drop` verb and no identity check on unclaim. To abandon a
task, unclaim then `bl close` (an empty worktree delivers nothing) — see `bl
close --skill`. To hand a held task to another agent, unclaim first, then the new
agent claims — a same-machine claim re-attaches the surviving branch, so
committed WIP is already in the new worktree.
