+++
title = "prime debris advice suggests re-claiming a closed ball; should detect closure and recommend only deletion"
created = 1784959154
updated = 1785124147
claimant = "windlass"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-e7f7"
on = "close"
+++
Observed 2026-07-24 on lernie: bl prime reported 'work/bl-04eb is committed but its worktree is gone — bl claim bl-04eb re-materializes onto it (a later close still delivers)' — but bl-04eb (and the twin bl-a785) were CLOSED balls; their content had landed via another ball's squash (bl-06d5) and the stale work/ branches were pure debris. The advice was wrong on both counts: the ball cannot be claimed (it no longer exists) and a later close cannot deliver. prime's debris report should check whether the task still exists in the store and, for a closed ball, say so and recommend only the branch-deletion arm (git branch -D), ideally after showing whether the branch's diff is contained in main.