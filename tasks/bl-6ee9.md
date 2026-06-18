+++
title = "bl-chore: generic guarded-mint-at-claim plugin (on-claim chore fanout)"
created = 1781809816
updated = 1781809867
priority = 2
tags = ["epic"]

[[blockers]]
id = "bl-3df3"
on = "close"

[[blockers]]
id = "bl-c370"
on = "close"

[[blockers]]
id = "bl-7b90"
on = "close"
+++
`bl-chore` — a generic, opt-in plugin that mints tagged gate children at `claim.post`, with the recursion and idempotency guards solved once so other plugins can sit on it.

## Why this exists

The discussed "on-claim subtask fanout" (auto-create gate tasks like "run the tests" when an agent claims) was decomposed long ago into the §10 blocker primitive: a gate is just a `--blocks close` subtask, and core only *enforces* blocking, never *mints* edges (the auto-mint rejection, §10). The *creation* side was left to plugins — forge mints its review gate at `claim.post` by hand-rolling `bl create` plus skip-own-gate-children logic (§11).

`bl-chore` factors that hand-rolled mint into ONE place. It is the **guarded-mint-at-claim primitive**:

> forge        = bl-chore (create the gate) + forge-sync (close it on PR merge)
> build gate   = bl-chore (create the gate) + build-plugin (close it on `make test`)

The recursion/idempotency guards get a single home instead of being re-derived per plugin. That is the subtraction that justifies shipping it in core rather than as a sibling repo.

## What it is (and is NOT)

- **Create-side only.** It mints children; it does not resolve them. A chore gate resolves when its file is gone — i.e. someone *closes* it (§10 "human approval = a child a person closes"). Resolution-by-mechanism (run `make test`, close on exit 0) is a SEPARATE, orthogonal plugin. Do not conflate.
- **Agentic, not enforcement.** "Run the test suite" is a forcing-function checklist the agent must discharge before `bl close` succeeds — not CI. If you want hard enforcement, that is what the repo's pre-commit hook is for (balls is intentionally agentic; the close-gate already runs the pre-commit hook, §11).
- **Shipped with core, NOT default-wired.** Like `tracker`/`bl-delivery` it ships as a default capability + reference implementation, but it must NOT be in the default `[hooks]` schedule — that would mint chores for every claim. You opt in: `bl conf prepend claim.post bl-chore` + write its config.

## Wiring & guards

- Children are minted `--blocks close` (gates). Per §10 a gate child does NOT affect readiness and never shows as a status, so chores cause zero ready-list clutter across N parallel claims; they surface exactly where wanted — at `bl close`, "blocked by: run the test suite."
- **Two predicates, distinct:**
  - *Tag-skip (ALWAYS, never knob-gated):* on `claim.post`, if the claimed task carries `bl-chore`'s own tag, bail. Breaks gates-for-gates recursion. Load-bearing because a chore is a leaf — the has-children check would not catch it.
  - *Epic-skip (KNOB, default on):* if the claimed task already has ANY children, bail. Keeps epics clutter-free AND gives idempotency for free (after firing once the parent has children, so a re-claim won't dup). The knob lives in the plugin's OWN config, never core surface (severability).
- **The plugin owns tagging its spawns.** The recursion break only holds if children `bl-chore` mints actually carry its tag — so the plugin injects the tag, callers never have to remember it. (A forge built on top that forgot the tag would silently re-open recursion.) This is the one interface invariant.

## Config (free-wheeling)

Free-wheeling by design — its config is a list of `bl` commands run at the stage; do whatever you want. The shipped SAMPLE is just the two base cases, as close-gates:

    bl create "Run the test suite"
    bl create "Review and update the docs"

(Each rendered with `--blocks close` against the claimed task + the injected tag.)

Note the default epic-skip means a leaf task you happened to give one real subtask gets no chores — the conservative call; the knob overrides. State this in the sample config's comment so it is not a surprise.

## Naming

`bl-chore` — first-party, consistent with the `bl-` = first-party convention (see bl-27bf). Sample config ships the two base cases above.

## Deliverable shape

Epic; children: design doc (living doc), implementation, integration test, doc update. Spec is frozen (§0–§17) so the doc-update child amends §6/§11 to register `bl-chore` as a shipped capability — fix the doc, don't deviate from it.