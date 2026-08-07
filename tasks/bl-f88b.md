+++
title = "bl-chore's claim scratch is never swept on the success path: one dead directory per claim, forever"
created = 1785824191
updated = 1786075161
claimant = "Odometers"
priority = 1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bug"]
+++
Observed 2026-08-03 in this checkout: 18 directories under
~/.local/state/balls/plugins/bl-chore/%2Fhome%2Fmark%2Fdev%2Fballs/<parent-id>/,
each holding a single `children` file of ~7 bytes naming the chore child that
claim minted. One per successful claim, never removed. 152K total, so this is
unbounded growth, not a disk problem.

Mechanism, from src/chore_scratch.rs. The scratch exists for exactly one
purpose: bl-ffbf rollback. `Minted::record` writes the minted child ids at
claim.post so that `Minted::unwind` can close them if the claiming op then
aborts. `unwind` ends with `fs::remove_dir_all(&self.dir)` — so the ROLLBACK
path cleans up after itself, and the success path never does. The module doc
says so outright:

  "A successful claim's record is inert bytes the next claim of that ball
   overwrites; only the rollback consumes it."

That is true and it is the bug: "inert bytes the next claim overwrites" only
holds for a re-claim of the SAME ball. A ball is normally claimed once, so the
common case is a directory that nothing will ever overwrite, read, or delete.

Why it is worth fixing rather than tolerating: the record is scoped to ONE op
invocation by design (the doc: "a rollback is scoped to ONE op invocation and
must not reach back into a claim that already succeeded"). Once that op seals,
the bytes are provably dead — a rollback for a sealed op cannot happen. So
the state has a well-defined end of life that nothing enforces.

Design question to attack before implementing, NOT pre-answered here:

1. Is there an "op sealed" signal a plugin can see? If claim.post already runs
   after the seal, the record could simply be deleted there once the mints are
   confirmed — and then the scratch never outlives its op at all, which is the
   subtractive answer (no sweeper, no new mechanism, the state just stops
   existing). Verify against the §14 rollback contract: if claim.post runs
   BEFORE the point an abort is still possible, deleting there breaks rollback.

2. If not, the fallback is a sweep with a provable predicate: a scratch dir
   whose ball is no longer claimed is dead. That is the same shape as
   delivery_prune.rs — enumerate the plugin territory, ask the store, delete
   what cannot be live. Costs a new call site and a store read the plugin may
   not have.

3. Do nothing and document it. 8KB/year of empty directories is arguably below
   the threshold that justifies mechanism. Include this arm honestly; the
   answer may be that the module doc should stop claiming the bytes are inert
   and admit they are litter.

Related but distinct: prime already prunes delivery worktrees and work/*
branches (bl-292d, src/delivery_prune.rs). bl-chore has no equivalent. Whether
that asymmetry is an accident or a correct consequence of the two plugins
holding different kinds of state is part of the question.