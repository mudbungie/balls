+++
title = "tasks_branch = landing is structurally impossible: one branch cannot have two worktrees"
created = 1781067175
updated = 1781067175
parent = "bl-72a8"
priority = 2
tags = ["spec-divergence"]
+++
SS1/SS2 promise the coincident case Just Works: "If tasks_branch names the landing, the two checkouts coincide on one branch whose config/ and tasks/ folders simply both live there - same code path"; "balls does NOT block on branch names".

Actual: config/ and tasks/ are realized as two worktrees of one local repo, and git forbids one branch checked out twice. Fresh checkout seeded with tasks_branch = "balls/config": first prime dies `fatal: balls/config is already used by worktree at .../config`; subsequent ops fail `cannot change to .../tasks: No such file or directory`.

Resolve per the freeze discipline, either direction: implement coincidence (a single shared checkout when the names are equal), or strike the promise with a SS15 revision entry and refuse `conf set task-branch <landing>` with a named error instead of wedging at prime.