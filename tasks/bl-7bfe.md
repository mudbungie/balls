+++
title = "Forge review is a subtask, not a delivery variant: guard stays at stage, PR submission is git-native work"
created = 1781043781
updated = 1781043781
priority = 2
tags = ["design"]
+++
## Why

The spec contradicted itself on close ordering, and the contradiction hid a deadlock. §9 numbered close.pre as "(1) delivery delivers, (2) core rejects on an open close-blocker" yet said the check is "evaluated before delivery and the seal"; §14's blocked-shape ("priors roll back; idempotent delivery effects survive via no-op rollbacks") required guard-AFTER-pre. The code runs closeable() at STAGE, before any plugin (src/lib.rs — enforce::close called from change at stage). Under guard-at-stage the §11 FORGE variant deadlocks: the PR is only opened by forge's close.pre, which never runs while the gate is open, so the gate can never resolve.

## Resolution (agreed 2026-06-09)

Resolve in the CODE's favor — guard stays at stage — by relocating the PR moment out of balls entirely:

- The review gate is an ORDINARY close-blocker gate child, minted by the forge plugin at claim.post (claim, not create: minting at create clutters non-deliverables and recurses on the plugin's own gate children — §10's "creation rides claim.post, the deliverable signal" reasoning stands).
- PR submission is GIT-NATIVE WORK: the worker pushes work/<id> and opens the PR themselves (title carries the [bl-id] tag). bl close stops being the submit verb; close is purely retire.
- A forge sync closes the gate child on merge. The forge plugin shrinks to two hooks: claim.post (mint) + sync (resolve).
- The parent's close then retires through the SAME direct delivery path: bl-430e's retry-idempotency already recognizes "a [bl-id] commit on integration since the fork" and skips the squash — a GitHub squash-merge is indistinguishable from a prior aborted close's delivery. The [bl-id] in the PR title is what makes the tag-scan arm fire (squash-merge rewrites commits, so the ancestry arm cannot). §11's FORGE delivery variant DISSOLVES; one delivery path, kind-blind.

## Costs, named

- An empty deliverable's review gate has no auto-resolve moment anymore (old design closed it at close.pre); its claimant closes it by hand ("nothing to review") — a skill-doc line.
- Abandon (unclaim ∘ close, bl-65e0) stays blocked by an open review gate: abandoning a forge-gated task means closing or --no-needs-unlinking the gate first — a skill-doc line.
- The sync join (merged PR → gate child) is plugin design: branch name work/<id> → parent → its open review blocker, or a preserved key on the gate.

## Scope (doc-only here; spec §15 entry included)

§9 close: swap the (1)/(2) order — the closeable() reject is at stage, before any plugin; strike "forge: idempotently pushes + opens/updates the PR". §11: collapse "Two variants" to one delivery path + rewrite the forge paragraph (claim.post mint + sync resolve + git-native submission). §14: blocked-shape becomes "core rejects at stage, before any plugin runs — nothing to unwind"; strike the forge close.pre no-op-rollback example (jira stays; the claim.post mint rollback — delete the just-minted gate child — survives in §11). §15: new entry with this rationale. Plugin rewrite tracked separately (balls-github-plugin repo; see sibling ball).