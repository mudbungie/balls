+++
title = "Multi-clone cross-tracking: a lagging clone half-closes + bl prime materializes bogus worktrees for already-closed tasks"
created = 1782610049
updated = 1782614953
claimant = "Demagogy"
priority = 3
tags = ["core"]
+++
Problem statement only (no solution proposed yet). Surfaced during the 0.5.4 backlog sprint, 2026-06-27.

When two checkouts share ONE center store via cross-tracking (e.g. balls core at ~/dev/balls and the adversary satellite at ~/dev/balls-adversary, both resolving the same `balls/tasks` on git@github.com:mudbungie/balls.git), each checkout has its OWN local store clone. Closing balls from both led to two distinct failures:

1. HALF-CLOSE ON A LAGGING CLONE. A `bl close` from clone A advances the shared remote `balls/tasks`. Clone B is then behind, so B's next close archival push is rejected non-fast-forward (`tracker: push rejected by the established remote store ... fetch first`). The CODE delivery to main STILL LANDS (squash committed), but the task stays `claimed` — an inconsistent half-closed state. A retry `bl close` AFTER `bl sync` completes the archival (the cosmetic `bl: No such file or directory (os error 2)` on retry is the already-torn-down worktree, non-fatal).

2. BOGUS WORKTREES FROM A LAGGING PRIME (the correctness bug). On the lagging clone, `bl list` shows already-closed tasks as `claimed`, and `bl prime` MATERIALIZES `work/<id>` worktrees (off the wrong repo's main) for tasks that are CLOSED on the remote. Cleanup required `git worktree remove --force` + `git branch -D work/<id>`.

Manual workaround during the sprint: `bl sync` the lagging clone before EVERY close/list. That should not be necessary. The prime-materializes-a-closed-task behavior in particular is a correctness defect. Design the fix separately; this ball just records the problem. See [[project_close_workflow]] / [[project_cross_tracking_trial]] in the worker's notes for the live repro.