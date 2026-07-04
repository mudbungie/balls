+++
title = "Root identity: match a ball's root_commit against the checkout's root SET (any-of admits)"
created = 1783197344
updated = 1783197399
claimant = "mark"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["core"]
+++
From CONVERGED docs/design/bl-0161-cross-repo-work.md (Edges, ratified 2026-07-04). Today `git rev-list --max-parents=0 HEAD` prints EVERY root of a multi-root repo and root_commit() takes the first line — so merging an unrelated history (vendoring) can reorder roots, flip the computed identity, and strand every earlier ball (root_commit is reserved, no repair edit). Fix: the claim guard's admit test matches the ball's recorded root against the SET of current roots — any-of admits. Strictly-more-correct identity read, tiny diff; makes 'vendored an unrelated history' a non-event. Create's stamp is untouched (first-line stays the canonical stamp; the SET is the read side). A true root REWRITE still orphans balls by design — identity IS the history. This is the foundation read for root-aware list (the follow-up ball builds its scope predicate on it).