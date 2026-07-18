# bl create — file a new task

    usage: bl create "TITLE" [--body B] [-p N] [-t TAG] [--parent ID]
             [--subtask-of ID] [--needs ID[:OP]] [--blocks OP|ID:OP]
             [-m MSG] [--as ID] [-- TITLE]

Files a task and prints its new id to **stdout** (so `id=$(bl create "…")`
captures it clean).

## Flags

- `--body B` — set the task's markdown body (its living document; overwrite it
  as the state changes).
- `-p, --priority N` — priority; higher sorts first in `bl list`.
- `-t, --tag TAG` — add a tag (repeatable; the flag is `--tag`, not `--tags`).
- `--parent ID` — containment only: builds the display tree, **gates nothing**.
- `--subtask-of ID` — child of ID *and* gate ITS claim (the everyday subtask
  spelling). Mutually exclusive with `--parent`.
- `--needs ID[:OP]` — add a blocker on this task (default `OP = claim`).
- `--blocks OP | ID:OP` — the reciprocal: gate ANOTHER task's op on this one.
  Create-only (it is an edge on another task, not this task's own field).
- `-m MSG` — commit note for the store journal.
- `--as ID` — worker identity.
- `--` — end option parsing (getopt). Shell an untrusted, `-`-leading title so
  it can't hijack a flag: `bl create -- "$TITLE"`.

## Examples

    bl create "Fix the parser" --body "repro: bl create -x"
    bl create "wire the auth endpoint" --subtask-of bl-1a2b
    bl create -- "$UNTRUSTED_TITLE"

## The dependency model

The one relational primitive is a blocker edge `{id, on}` on the *blocked* task:
"this task can't do op `on` until task `id` resolves." `on` is ANY op; two have
create-time sugar:

- `--needs B[:OP]` — a blocker on THIS task (default `claim`, i.e. can't be
  claimed until B closes).
- `--blocks OP` / `--blocks ID:OP` — the reciprocal on ANOTHER task. `--parent X
  --blocks close` gates X's close on this task.
- `--subtask-of E` — sugar for `--parent E --blocks claim` in one word: child of
  E, and E can't be **claimed** until this closes. Gating claim (not close) is
  what keeps an epic-with-open-children out of `bl list -s ready` — the epic
  derives as *blocked* per open child, so a dispatcher never lands an agent on an
  unactionable container; the last child closing auto-readies the epic. Prefer
  this over bare `--parent` when filing subtasks: the gate rides in the flag's
  name, so it can't be silently forgotten.

**Every edge target must be LIVE.** `--needs`/`--blocks`/`--subtask-of` refuse a
target id that is unknown or already closed, naming which — a never-minted id is
a typo or hallucination (it would leave the task silently ungated), and a dead
blocker can never block. The remedy is dropping the flag.

**No cycles through claim/close.** The same flags refuse an edge that would
close a loop over the lifecycle ops, naming it (`bl-a -claim-> bl-b -close->
bl-a`): a ball resolves by closing, so no claim→close order can resolve every
ball on such a loop — and `bl list` would render the pair as a healthy
ready/blocked right up until `bl close` refuses with the work already done
(bl-54fe). The classic mis-wiring is a verification gate spelled BOTH ways —
`--parent X --blocks close` *plus* `--needs X`. A gate is ONE edge; pick the
direction:

- `--needs X` alone — the gate becomes claimable once X delivers
  (post-delivery verification).
- `--parent X --blocks close` alone — X can't close until the gate does; but
  the gate is claimable immediately, against a `main` that does not yet carry
  X's work, so it can only verify what already landed. True pre-merge
  enforcement is the repo's own `pre-commit` hook, which `bl close` already
  runs.

Unlink a mis-wired edge with `bl update <id> --no-needs <id>` — the unlink is
never refused.

`--parent` is **containment only** — it builds the display tree and gates
nothing. An "epic" is just a task with children; to make a parent wait on its
children, add explicit edges (`--subtask-of` at create is the usual way).

**Splitting work? Wire the gates.** Filing N balls with no edges declares them
fully independent — and since one agent is dispatched per UNBLOCKED ball, that
reads as "claim all N in parallel, now," colliding on shared files. If the
pieces have an order, encode it with `--needs`. Filing flat IS a parallelism
decision. The order lives in the **ball graph** (what the dispatcher reads),
never only in a design doc or commit note.

To edit a task's own blockers after create, see `bl update --skill`
(`--needs` / `--no-needs`). Reciprocal `--blocks` stays create-only.
