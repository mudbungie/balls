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
- `--subtask-of ID` — child of ID *and* gate ITS close (the everyday subtask
  spelling; it also nests delivery — see below). Mutually exclusive with
  `--parent`.
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
- `--subtask-of E` — sugar for `--parent E --blocks close` in one word: child of
  E, and E can't be **closed** until this closes. Prefer it over bare `--parent`
  when filing subtasks: the gate rides in the flag's name, so it can't be
  silently forgotten.

Gating E's close (not its claim) is what makes E an **epic that exists in git**:
the pair — a parent pointer *plus* a close-gate on that same parent — is the
nesting declaration, so this subtask forks and delivers into `work/<E>` instead
of the integration branch, and E lands the accumulated work as one commit when
it closes (`bl close --skill`, "the delivery target"). The order it implies is
"E is claimed first (or never), children deliver into it, E retires last".

The cost, stated plainly: an epic with open subtasks now derives **ready**, so
it does show up in `bl list -s ready` — it is claimable, because claiming it is
how you make integration edits on the branch its children are landing in. What
you get for that is enforcement the old claim-gate never had: `bl close` on the
epic is **refused** while a subtask is open, naming it. (Before 2026-07-21 the
sugar minted `--blocks claim` instead, which kept epics out of the ready set but
gated nothing at close. Balls filed under the old sugar carry a claim-gate and
no close-gate, so they keep flat delivery and behave exactly as they always did
— nothing needs converting.)

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
`--parent X --blocks close` (or its one-word spelling `--subtask-of X`) *plus*
`--needs X`. A gate is ONE edge; pick the direction:

- `--needs X` alone — the gate becomes claimable once X delivers
  (post-delivery verification).
- `--parent X --blocks close` alone (= `--subtask-of X`) — X can't close until
  the gate does, and the pair is also the **nesting** declaration: the gate's
  worktree forks X's
  live branch and its close delivers back into it, so it verifies the work it
  gates rather than a `main` without it (`bl close --skill`). It stays claimable
  immediately, so it can still be picked up mid-flight, before X's work exists.

Unlink a mis-wired edge with `bl update <id> --no-needs <id>` — the unlink is
never refused.

`--parent` is **containment only** — it builds the display tree and gates
nothing. An "epic" is just a task with children; to make a parent wait on its
children, add explicit edges (`--subtask-of` at create is the usual way). It is
also containment that licenses nested delivery: `--parent X --blocks close`
(i.e. `--subtask-of X`) makes X the child's delivery target, while a bare
`--blocks X:close` on a NON-parent is pure ordering and keeps both balls
delivering independently.

**Splitting work? Wire the gates.** Filing N balls with no edges declares them
fully independent — and since one agent is dispatched per UNBLOCKED ball, that
reads as "claim all N in parallel, now," colliding on shared files. If the
pieces have an order, encode it with `--needs`. Filing flat IS a parallelism
decision. The order lives in the **ball graph** (what the dispatcher reads),
never only in a design doc or commit note.

To edit a task's own blockers after create, see `bl update --skill`
(`--needs` / `--no-needs`). Reciprocal `--blocks` stays create-only.
