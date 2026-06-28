+++
title = "Claim guards on the project's root-commit hash — reject wrong-repo claims (bl-c3c0 fix 2)"
created = 1782631094
updated = 1782633350
priority = 3
tags = ["core"]

[[blockers]]
id = "bl-c2bf"
on = "claim"
+++
Per docs/design/bl-c3c0-lagging-clone.md (Fix 2). Record the project's root-commit hash (git rev-list --max-parents=0 <branch>) on a ball at create (Task::extra seam or first-class field). On claim, compute the current checkout's root and REJECT on mismatch, naming the repo the ball belongs to (Project::at already shells git against the code repo, src/delivery_repo.rs). NO override/escape-hatch — a same-root collision fails open and grants nothing (the attacker would already be working from that directory). Back-compat: a ball with no recorded root is unconstrained; a ball from a checkout with no code repo records nothing. Companion to the prime-deletion ball (bl-c2bf): guards the claim-side wrong-repo hazard (worktree off the wrong main). Canonical identity is remote-free.