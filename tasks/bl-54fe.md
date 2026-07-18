+++
title = "Gate wiring is a footgun: --blocks close yields vacuous gates, and the obvious fix silently deadlocks"
created = 1783830492
updated = 1784337687
claimant = "Dickers"
priority = 4
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bug", "ergonomics"]

[[blockers]]
id = "bl-c8f8"
on = "close"
+++
Agents wire parent/gate task sets **wrong by default**, and bl neither prevents it nor shows it. Observed live in `/home/mark/dev/lernie` on 2026-07-11 (5 parent tasks x 3 gate subtasks = 15 tasks, all mis-wired). Reported as a recurring failure, not a one-off.

## The setup

A common repo convention (lernie’s `CLAUDE.md`) says: *"When creating a task, always create the following gates: tests / docs / alignment."* A gate means "you may not merge until this is verified."

An agent reaching for that spelling finds it documented in `bl create --skill`:

> `--blocks OP | ID:OP` — the reciprocal: gate ANOTHER task’s op on this one. … `--parent X --blocks close` gates X’s close on this task.

So the agent writes `bl create "tests: ..." --parent X --blocks close`. That is the spelling the docs advertise for exactly this intent. It produces two silent defects.

## Defect 1 — the gate is vacuously satisfiable

`--parent X --blocks close` gives the **parent** a blocker `{gate, on: close}` and gives the **gate** no blockers at all. So the gate derives as `ready` the moment it is filed.

Any agent running `bl list -s ready` can claim `tests: coverage 100% and all tests pass`, get a worktree off a clean `main`, run the suite, watch it pass, and close it — **with the parent’s work not yet written.** The parent’s close-gate is now satisfied without the parent’s change ever having been tested.

The `--blocks close` edge therefore provides **zero real protection**. It looks like a gate and enforces nothing.

## Defect 2 — the obvious fix deadlocks, and `bl list` hides it

The natural correction is "a gate should not be claimable until its parent’s work exists":

    bl update <gate> --needs <parent>      # gate blocked on claim until parent resolves

Combined with the pre-existing close-blocker, that is a **cycle**:

- parent cannot **close** until gate closes (`{gate, on: close}`)
- gate cannot be **claimed** until parent closes (`{parent, on: claim}`)
- gate cannot close without being claimed → nothing can move

**`bl list` shows this as healthy.** Because the parent’s blocker is on `close` and not on `claim`, the parent still derives `ready`; the gate derives `blocked`. That reads exactly like a correct epic/subtask pair. Verified empirically:

    ready    bl-65d8  Design: dissolve await ...
    blocked  bl-40ae  tests: coverage 100% and all tests pass

The trap springs at `bl close`, **after the agent has done all the work** — refused, with no legal way forward, because the thing that would unblock it is itself blocked on the refusal.

bl knows cycles exist and does nothing about them. `bl update --skill`:

> `--needs ID[:OP]` / `--no-needs ID` — add or unlink one of THIS task’s own blockers (**the in-band fix for a mis-wired or cyclic blocker**).

An in-band fix is offered; detection is not.

## Root cause

A gate wants: **claimable once the parent’s work exists, but before the parent delivers.** That state is unexpressible. A task has exactly one resolution (close), and close *is* delivery (`bl close` squashes to `main`), so there is no edge to hang "work done, not yet delivered" on.

Agents therefore approximate, and both approximations are wrong: `--blocks close` gives a gate that enforces nothing, and `--needs parent` gives a deadlock. The mistake is not carelessness — it is that the primitive cannot say the thing the convention asks for, and the docs advertise the broken spelling.

## Candidate fixes (not prescriptive)

1. **Cycle detection at `create`/`update`.** Refuse, or loudly warn, when a new blocker edge would close a cycle across ops. Catches the mistake at the moment it is made rather than at close. Cheapest fix with the highest payoff.
2. **`bl list` must not render a deadlocked pair as healthy.** A task whose `close` is blocked by a task whose `claim` is blocked by it is a hard error, not a `ready`/`blocked` pair.
3. **Reconsider `--blocks close` entirely.** It is the spelling that leads agents into defect 1, and its only documented use case (gates) is the one it serves worst.
4. **Name the correct topology in the skill.** For a work-carrying parent with verification gates, the non-cyclic wiring is: gate `--needs parent` (claim-blocked), parent carries **no** close-blocker — verification is post-delivery, and true pre-merge enforcement lives in the repo’s `pre-commit` hook (which `bl close` already runs). If that is the intended pattern, `bl create --skill` should say so where it currently advertises `--parent X --blocks close`.
5. **Or give gates a first-class primitive** — a checklist that gates close without being a claimable task.

## Repro

    bl create "work" ; bl create "gate" --parent <work> --blocks close
    bl list                     # gate is READY — claimable, vacuously closeable
    bl update <gate> --needs <work>
    bl list                     # looks healthy: work=ready, gate=blocked. It is deadlocked.

## Fix applied downstream

lernie’s 15 gates were rewired to option 4 (gate `--needs parent`, close-blockers removed). Filed here because the next agent to follow the documented spelling will make the identical mistake.

## Resolution (2026-07-17)

Shipped candidate fixes **1 + 4**; 2 dissolves into 1 (a cycle that can never be written never needs rendering), 3/5 are bl-7b71's territory.

- **Write-time cycle refusal** (`enforce::acyclic`): the front-door edge flags (`--needs`/`--blocks`/`--subtask-of`, create and update) refuse a new edge closing a loop over the lifecycle ops (claim/close), naming the full loop and the one-edge topology in the message. Only those two ops count — a ball resolves by closing, so an edge on any other op can never strand a loop. Write-side only, per the §10 live-target precedent: pre-existing cycles never refuse unrelated edits, `--no-needs` always passes, `--edit`/`import` stay verbatim escape hatches.
- **Docs name the correct topology**: skill/create.md "No cycles through claim/close" (gate is ONE edge — `--needs parent` for post-delivery verification, or `--blocks close` alone knowing it verifies only what already landed; pre-merge enforcement is the repo pre-commit hook close already runs); architecture §10 deadlock paragraph rewritten — the old "links are mutable, unlink to fix" stance mis-predicted where the trap springs (claim never refuses; the refusal lands at close, after the work).
- **Why not more**: the unexpressible state ("claimable once the parent's work exists, before it delivers") stays unexpressible here — that is bl-7b71's nested-delivery design (target ref = parent's work branch), under which `--parent X --blocks close` becomes correct as written. Cross-referenced, not gated.