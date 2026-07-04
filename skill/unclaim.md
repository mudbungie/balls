# bl unclaim — release a claim

    usage: bl unclaim <id> [--as ID] [--remote URL]

Releases your claim on a task and removes its `work/<id>` worktree.

## Flags

- `--as ID` — worker identity.
- `--remote URL` — per-op store remote (see `bl prime --skill`).

## Examples

    bl unclaim bl-1a2b

## What survives

Removing the worktree discards **uncommitted** work — it dies with the worktree.
Work you **committed** on the `work/<id>` branch **survives**: a later `bl claim`
+ `bl close` delivers it. To discard that committed work too, delete the branch
explicitly: `git branch -D work/<id>`.

There is no separate `drop` verb and no identity check on unclaim. To abandon a
task, unclaim then `bl close` (an empty worktree delivers nothing) — see `bl
close --skill`. To hand a held task to another agent, unclaim first, then the new
agent claims (cherry-pick preserves any committed WIP across the takeover).
