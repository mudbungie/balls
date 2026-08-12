+++
title = "Merging queue: seal-by-tag ordering + monotone landing"
created = 1786515683
updated = 1786516036
claimant = "Gushed"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["merge-queue"]

[[blockers]]
id = "bl-39ad"
on = "close"
+++
Queue semantics for the speculative merge queue (design: docs/design/bl-24e7-speculative-merge-queue.md). Order is a QUERY over merging-tags — tag-time is position; no stored queue, nothing to desync. One invariant covers seal/eviction/requeue: a branch is sealed while tagged; any new commit requires dropping the tag and re-tagging (so a gate-failing culprit structurally cannot fix in place — fixing means committing means retagging means bottom). Also settle open Q1, the monotone landing rule — proposal in the doc: land only the longest prefix in which every shorter prefix also passed; a pass above a failure is masking, not endorsement. WHY: deriving order from tags keeps single-source-of-truth (status is derived, never stored — same doctrine as the rest of balls), and the seal invariant kills the stale-snapshot failure mode that sank cross-pull designs.