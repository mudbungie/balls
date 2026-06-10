+++
title = "skill note: non-bare delivery leaves the project working tree stale (add -A footgun)"
created = 1781067253
updated = 1781068988
claimant = "Rebut"
parent = "bl-72a8"
priority = 3
tags = ["docs"]
+++
The delivery squash moves the integration ref by plumbing and never touches a NON-BARE project root working tree or index. After a close, the root checkout is stale relative to its own branch; a later `git add -A && git commit` there silently commits DELETIONS of every previously delivered file (reproduced during verification: one commit deleted four delivered files).

Design-consistent - bare is the documented deployment norm, the squash is plumbing by design (bl-ee85), and editing main outside the worktree is already forbidden by the skill guide - but the trap is real for non-bare users and currently undocumented. Add a skill-guide warning (work in the bl worktrees; after a close in a non-bare repo, `git checkout`/reset the root before touching it). Do NOT auto-reset the user tree from the plugin - never touch state the user may have dirtied.