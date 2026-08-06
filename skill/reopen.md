# bl reopen — restore a closed task from history

    usage: bl reopen <id> [--clean] [-m MSG] [--as ID] [--remote URL]

Brings a retired ball back: restores `tasks/<id>.md` with the content it had the
instant before it was closed, and seals that restore onto the store.

## Flags

- `--clean` — restore the ball **fresh**: drop the `claimant` the close left on
  it. Off by default; see "What `--clean` does" below.
- `-m MSG` — commit note for the store journal.
- `--as ID` — worker identity.
- `--remote URL` — per-op store remote (see `bl prime --skill`).
- `-C PATH` — **global** (every command): address the store keyed by PATH, as
  if `bl` had run there. No walking, no git-root discovery.

## Examples

    bl reopen bl-1a2b
    bl reopen bl-1a2b --clean -m "the fix regressed; picking it back up"

## Why this is not a new state

A closed ball is not a tombstone — it is **older content**. Close deletes
`tasks/<id>.md`, and every dead-ball read (`bl show <closed-id>`, `bl list -s
closed`, `bl list --all`) already reconstructs it by walking `balls/tasks`
history to the newest commit that deleted the file and reading its parent tree.

`reopen` is the **write half of that same walk**. It restores exactly the content
`bl show <id>` would have printed. An id is a sequence of incarnations with **at
most one live at a time**, which needs no enforcing: there is one path,
`tasks/<id>.md`, so a restore cannot produce two.

Preview what you are about to restore with `bl show <id>` first — same content,
same walk.

## What `--clean` does

**Nothing implicitly.** A bare `bl reopen` restores the frontmatter verbatim:
`created`, `priority`, `tags`, `parent`, and the ball's `blockers` all come back
as they were, because they are still your declarations and the tool has no
business editing them. `updated` is restamped, as on every op.

`--clean` drops exactly one field: `claimant`. That is the only field a close can
falsify — it named a `work/<id>` worktree that the close then tore down, so a
verbatim restore hands you a ball claimed by an agent that no longer holds
anything. Use `--clean` when you want the ball back on the ready list; omit it
when you are restoring a record and want it byte-faithful.

You can always fix it afterwards instead: `bl unclaim <id>` clears the same
field.

## Refusals

**A live id.** Closed ids are legally re-minted — `bl create` re-rolls only
against ids that are live *now* — so `bl-1a2b` may today be an unrelated ball.
Reopening would clobber it, so a live id is refused outright. Read it with `bl
show <id>` to see which ball you actually have.

**An id that names nothing.** No live ball and no `tasks/<id>.md` deletion
anywhere in this store's history.

Both refusals land before any worktree is made or any plugin runs.

## What reopen does NOT do

**It does not un-deliver the code.** `bl close` squashed `work/<id>` onto the
delivery target before it archived the task; reopening the ball says nothing
about that commit and does not revert it. If you want the code gone, that is an
ordinary `git revert` — a separate, deliberate act.

**It does not reopen mirrors.** A forge plugin that closed a GitHub issue on
close does not reopen it here (`gh issue reopen` is yours to run), and a
`claim.post` chore plugin will mint a *fresh* child on your next claim rather
than resurrecting the old one. Those are plugin-level facts, not core's.

## Then what

Reopen restores the ball; it does not claim it. `bl claim <id>` next, as usual.

If `work/<id>` still exists on this machine, the claim **reattaches** it — your
old branch and its commits come back with the worktree, already delivered to the
target once. Usually it does not exist: `bl prime` prunes settled `work/*`
branches, so the claim forks a fresh branch from the current integration tip.
Nothing counts closes per ball — a second close on the same id is ordinary.

Reopen is gated like any other op: a blocker on this ball whose `on` is `reopen`
refuses it (`bl create --skill`, the dependency model).
