# bl close — deliver the work and archive the task

    usage: bl close <id> [-m MSG] [--as ID] [--remote URL]

Delivers your work and retires the task in one move: **fold `main` in → run the
repo's pre-commit hook → squash `work/<id>` to `main`**, then archive the task
and tear down the worktree.

## Flags

- `-m MSG` — commit note for the store journal.
- `--as ID` — worker identity.
- `--remote URL` — per-op store remote (see `bl prime --skill`).
- `-C PATH` — **global** (every command): address the store keyed by PATH, as
  if `bl` had run there. No walking, no git-root discovery.

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

Concurrent closes in one checkout are safe: the delivery ref move is a
compare-and-swap, so if a sibling close lands on `main` mid-delivery the loser
aborts loudly (nothing overwritten) — just re-run `bl close` to deliver onto the
new tip.

The delivery commit lands on `main` tagged `[bl-xxxx]` — that tag is how a merge
is recognized as the task's delivery.

## Where "main" actually is: the delivery target

`main` above is the **default** target, not a constant. A task delivers to the
ref it targets, derived per op and never stored:

- it close-gates its **live parent** (`--parent X` *and* `--blocks close` — one
  word, `--subtask-of X`; see `bl create --skill`) ⇒ its target is `work/<X>`,
  the parent's own branch;
- otherwise ⇒ the repo's integration branch (whatever HEAD points at, usually
  `main`).

So an epic accumulates its children on `work/<epic>` and lands them as ONE
commit when the epic itself closes — main is simply what a parentless ball
targets. Everything above holds unchanged at every depth: the fold, the
pre-commit gate and the tagged squash all run against *that ball's* target, so
a child that breaks the gate fails at its own close, in its own worktree.

Two consequences worth knowing:

- **A closed child is delivered, not landed.** Its work is on the epic's ref,
  not on main, until the epic closes. Whether a ball's work is on main is a git
  question, as it always was: `git log --grep '[bl-xxxx]' main`.
- **Any checkout of a moved ref is stale.** A delivery advances a ref by
  plumbing and never touches a checkout of it — that is the non-bare root after
  a close, and equally an epic's own worktree after a child closes into it.
  Refresh before working there.

Deleting a live epic ref (`git branch -D work/<epic>`) discards the delivered
work of every child that closed into it. `bl` never does that — prune deletes
only settled branches — but you can.

## Close refuses a task file you haven't seen

The task file IS the contract close seals. If it changed since **your own last
touch of it** (claim counts, so a claimant always has one) and nothing shows you
saw the change, close refuses and **prints the unseen diff** — then a bare
re-run of the same `bl close` passes and seals exactly that content (the
refusal itself acknowledges the diff it just put on your stdout). If yet
another edit lands in between, it refuses again with the new diff:
compare-and-swap semantics, worst case one refusal per unseen edit. Running
`bl show <id>` after the foreign edit also counts as having seen it — the
close then passes first try. Your own edits never trigger this.

Close does **not** push the code remote; pushing `main` is your own deliberate
`git push`.

## Closing IS the only retirement

A closed task has **no file** (absence = resolved); its history is the record.
To abandon a held task, `bl unclaim` then `bl close` — an empty worktree
delivers no code, so a `close`-gate guards every way a task can die.

Closing an **epic** is therefore gated by its subtasks: `--subtask-of E` mints a
close-blocker on E, so `bl close E` is refused while any subtask is open, naming
it. Children wired with bare `--parent` gate nothing — a close that leaves only
those succeeds and prints a notice ("closed with N open children, none gating"),
informational and never a block; they survive with dangling, display-only parent
pointers.

## Submit/approve flows

The default is solo: the agent that claims also closes. For a split flow, add a
review gate as an ordinary close-blocker subtask (`bl create "review X"
--subtask-of X`, or a forge plugin that mints one at claim — the gate then forks
and delivers into X's own branch, per the target rule above). Submission is
git-native — push the work branch and open the PR yourself with the `[bl-id]`
tag in the PR title so the merge is recognized as the delivery.
