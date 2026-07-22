+++
title = "Per-task delivery target: core derives, one wire field, plugin consumes"
created = 1784690289
updated = 1784691343
claimant = "Betides-Core"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["nested-delivery"]

[[blockers]]
id = "bl-439d"
on = "close"
+++
Implements the mechanism half of docs/design/bl-7b71-nested-delivery.md (CONVERGED; read it first — it argues every choice below).

**The rule.** A task delivers to a target ref derived at op time, never stored: if it close-gates its live parent (--parent X plus a {this, on: close} edge on X) the target is work/<X>; otherwise the integration branch. claim forks the target, close folds the target in, runs the repo pre-commit hook on the folded tree, and plumbing-merges back to it. Flat delivery is the degenerate case — main is what a parentless ball targets. Depth recurses for free.

**The seam.** The derivation is a graph fact; bl-delivery is kind-blind and cannot see the gating edge (it lives on the PARENT task file, not on the ball riding the wire). So core derives and passes it:

- add `target: Option<String>` to the §7 `Command` (src/wire.rs) — the ID of the ball whose branch this op targets; `skip_serializing_if = "Option::is_none"` so an absent target leaves every existing payload byte-identical (the `stealth: bool` precedent, bl-9df0).
- it carries an ID, not a branch name: `work/<id>` is the plugin formula (delivery_path), and core spelling it would be a second home for the naming.
- the plugin rule becomes `target.map(work_branch).unwrap_or(integration()?)`. `Repo::integration()` (git symbolic-ref --short HEAD) survives as the DEFAULT, not a rival — it is not and never was hardcoded to "main".

**Deliberately unchanged.** prime pruning: it keeps a work/<id> branch whose delivery is not yet contained in the integration branch, so a child closed into an epic is simply unsettled until the epic lands, then settles and prunes with zero new logic. The existing conservative test is already the correct nested test.

**Uniform hook.** Every close runs the repo pre-commit gate against its own target, children included — attribution: breakage fails in the worktree that caused it, not at whoever closes last. The root run stays non-redundant (two children can each pass alone and fail merged).

**Coverage.** The derivation is pure graph arithmetic over (task, parent-task) — unit-testable in src without a temp repo, like the existing dispatch matrix against the fake Repo. The end-to-end nested close belongs in tests/ (tarpaulin ignores tests/, so it is coverage-neutral).

Does NOT include: the --subtask-of claim-gate to close-gate flip, or the rendered target column. Both are separate balls needing this one.