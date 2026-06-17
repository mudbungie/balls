+++
title = "subtask-of default gates claim, not close (epics drop out of ready)"
created = 1781727735
updated = 1781727735
+++
PROBLEM
Agents file an epic with subtasks but the epic surfaces as `ready` and gets
claimed though it is unactionable — the work is the children. `bl ready | head
-1 | xargs bl claim` lands a context-free agent on a container with nothing to do.

ROOT CAUSE
`--subtask-of E` expands to `--parent E --blocks close` (skill: "Blockers and
the dependency model"; architecture.md ~§9). That gates the epic's CLOSE but
not its CLAIM, and status derivation is "blocked = unresolved CLAIM-blocker
ONLY" (task.rs Task::status) — a close-blocker yields NO blocked status. So the
epic reads `ready`.

DECISION
Change the `--subtask-of E` default wiring from `--blocks close` to
`--blocks claim`:

    --subtask-of E  ==  --parent E --blocks claim

The epic then has an unresolved claim-blocker per open child -> derives as
`blocked` -> excluded from `bl list -s ready` (= `bl ready`). When the last
child closes, the claim-blockers resolve by file-existence (no teardown) and the
epic flips blocked -> ready automatically.

WHY THIS IS NOT DOUBLE-WIRING (the smell that blocked us)
Today's `--subtask-of` ALREADY lays down one edge per child on the epic (the
blocker is stored on the blocked task). N subtasks = N close-edges now. This is
not "add a claim-edge on top of the close-edge" — it is the SAME edge count,
with the `on` op swapped. The redundant edge was `close`, not "a second edge".

WHY DROP THE CLOSE-GATE (no lifecycle enforcement, by design)
The close-gate was core refusing to retire the epic until children resolve —
i.e. lifecycle enforcement, which we explicitly do NOT want. close does not
require a prior claim (change.rs Retire::stage checks only enforce::close;
abandonment is unclaim-then-close), so a claim-gate never *enforced* close
anyway. The goal is the PAVED PATH: agents do not close what they did not claim,
so gating claim keeps them off the road to a premature epic close behaviorally,
with no rule added. The stray `bl close E` on an unclaimed epic is an off-path
case we have decided not to police.

STORIES VALIDATED
- New epic+subtasks: agent uses --subtask-of, claim-gate wired, can't forget.
- `bl ready | head -1 | xargs bl claim`: gated epic excluded, lands on a leaf.
- Subtask under already-claimed epic: claim-edge inert (rung 1 moots it) — fine,
  the orchestrator holds it and is on the path.
- Orchestrator bypass: claim the epic up front; occupancy moots the claim-edges.
  No --force flag (and none wanted).
- Last child closes: epic auto-readies, no manual edge removal.
- Nested epics: per-level wiring gates each immediate parent transitively.

RESIDUAL (known, accepted)
Re-parenting a child (`update --parent`) does not rewrite the epic's edges — a
stale claim-edge on the old epic, none on the new. Inherent to ANY explicit-edge
scheme. The only thing that removes it is deriving the gate from the `parent`
pointer (zero edges) — set aside for now; that is the door back if stale edges
ever bite in practice.

SCOPE OF THE CHANGE
- The `--subtask-of` create sugar: emit a claim-blocker instead of a close one.
- architecture.md §9/§10: the `--subtask-of E = --parent E --blocks close`
  verbiage (the everyday-bundle line) -> `--blocks claim`.
- `bl skill` text ("Blockers and the dependency model" + the "make a parent wait
  on its children" note) and `bl help` if it cites the expansion.
- Tests asserting --subtask-of produces a close-edge.