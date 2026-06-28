+++
title = "Delete prime's worktree re-materialization — materialize at claim only (bl-c3c0 fix 1)"
created = 1782631081
updated = 1782635159
claimant = "Broadest"
priority = 2
tags = ["core"]
+++
Per docs/design/bl-c3c0-lagging-clone.md (Fix 1). Delete the claimed_ids worktree-materialization loop in bl-delivery's prime.post (src/bin/bl-delivery.rs:120-126); KEEP repo.prune() at :128 as housekeeping. Remove the prime path-print in delivery::surfaced and the skill/docs line about prime re-materializing held worktrees. Worktrees materialize at CLAIM only; claim stays non-idempotent; re-priming a lost worktree is unclaim+claim. This is the fix for the recorded bug (prime makes bogus worktrees for closed/wrong-repo tasks on a lagging clone) by pure subtraction — it moots all sync-ordering concerns. Shippable independently of the other two bl-c3c0 balls.