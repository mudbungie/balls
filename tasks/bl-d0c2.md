+++
title = "Speculator: merge-tree prefix builds + eagerness scheduling"
created = 1786515683
updated = 1786516166
claimant = "Gushed"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["merge-queue"]

[[blockers]]
id = "bl-1263"
on = "claim"

[[blockers]]
id = "bl-5c5f"
on = "claim"
+++
The engine (design: docs/design/bl-24e7-speculative-merge-queue.md). Candidates are queue PREFIXES ONLY (N candidates, never other combinations), computed by git merge-tree — no branch, no index, no worktree, so the ref-debris class never exists; a conflicted merge-tree result marks the candidate unbuildable and the ball falls back to fold-at-close (settles open Q2 — resolution is judgment, judgment belongs to the branch owner). Build = check the tree into a scratch dir, run the staged gate (clippy -> 300-line -> tarpaulin; cheap stages for ALL candidates up front — evictions announce in the first minute), write the verdict record. Scheduling: capacity is MEASURED (concurrency cap from cores/memory), preference is DECLARED (one scalar S: start expensive stage when slack <= S x build time; inf=server-eager, ~1=laptop-JIT, 0=off; default S from AC/battery state); nice by queue position; close-time builds on cache miss run unniced and preempt. One persistent build dir per agent (warm incremental builds), swept by prime debris pass. Testable cleanup invariants: zero speculation refs (there are none by construction), worktree list shows only real claims, scratch dirs gone after each round. WHY: parallel prefix gates are built-in bisection — first failing prefix names the culprit inside the same gate window — and depth-reluctance emerges from cap+priority rather than a policy table.