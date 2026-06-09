+++
title = "Retire the 'dropped' status semantic — collapse to 'closed'"
created = 1780965575
updated = 1780966697
claimant = "Imprudence"
priority = 3
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls/bl-23eb"
+++
Reductive: dropped is just a subset of closed. Both are *retirement* — the task file is deleted from balls/tasks; the only difference is the deleting verb (bl-op: close vs drop), which src/reads/history.rs surfaces as two derived statuses via Retired::from_op.

Collapse it: the derived status for any dead ball is **closed**, full stop. The Retired enum loses its close/drop split (becomes a unit / goes away); list + show render 'closed' for both. No separate 'dropped' word in any human render.

The `bl drop` VERB stays — it's the abandon-without-delivery path (skips the close.pre delivery/forge gates that close runs). What changes is only the projection: a drop-retired ball reads as 'closed'. The `bl-op: drop` trailer remains in git as bedrock history (it can't and needn't lie); it's simply no longer projected as a distinct status.

--json is unaffected: it's the bedrock raw-frontmatter projection and never carried a derived status. Scope: src/reads/history.rs (Retired), list/show render, SKILL.md ('A closed or dropped task…' → 'A closed task…'), tests asserting a 'dropped' rung.