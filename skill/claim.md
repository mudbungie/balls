# bl claim — take a task and materialize its work worktree

    usage: bl claim <id> [--as ID] [--remote URL]

Starts work: takes occupancy of the task and materializes its `work/<id>` code
worktree off `main`. Prints the **worktree path to stdout** — the verb's one
product, and the one moment a worktree materializes.

## Flags

- `--as ID` — worker identity (**always pass this**; see below).
- `--remote URL` — per-op store remote (the top of the resolution ladder; see
  `bl prime --skill`). Not remembered.

## Examples

    bl claim bl-1a2b --as alice

## The worktree is the unit of work

`bl` tracks the code change a task delivers, not just the task. While you hold
the claim, **all edits go in that worktree**, never on `main` directly. Editing
`main` outside the worktree bypasses the lifecycle: `bl close`'s delivery squash
captures the worktree's diff, so a stray `main` edit is invisible to it — the
task closes cleanly while leaving your change behind, undelivered.

The path is **computed, never stored**: `bl show --json` (the lossless mirror of
stored frontmatter) never carries it, because it is machine-local. If you lose
the printed path, `git worktree list` (the `work/<id>` line) reads it back, and
`bl show <id>` (human view) folds a `worktree` line in when the worktree exists
on this machine. Lost the worktree itself? Re-make it with `bl unclaim` then `bl
claim`.

## Identity

`--as ID` stamps the claim with a worker identity (else `$USER`, else the
literal `"unknown"`). Do not let an LLM invent its own name — models collapse to
the same few names across sessions and step on each other's claims. Have the
harness pick a name at session start and pass it as `--as`.

## Blocked tasks

A `claim` of a task with an unresolved `claim`-blocker is refused, naming the
blocker. That is by design — `bl list -s ready` only shows claimable tasks. To
change a task's blockers, see `bl update --skill`.
