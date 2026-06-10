+++
title = "materialize never converges a repointed tasks_branch: store.exists() early return"
created = 1781067143
updated = 1781067143
parent = "bl-72a8"
priority = 1
tags = ["bug"]
+++
src/substrate.rs materialize() returns Ok immediately when the store dir exists, never comparing the checkout branch to the configured tasks_branch name. Convergence is "a store dir exists", but the SS12 predicate is "the store (tasks_branch) resolves to a valid, CURRENT tasks/ checkout".

Reproduced symptoms, all on a once-primed checkout:
- `bl conf set task-branch X; bl prime` exits 0 silently and every op keeps using the old branch (reads AND writes).
- `bl prime --install <center>` adopts the config and even clones the new ref in, but `bl list` still reads the old store (the adopted name is never brought to readiness).
- `bl install ... --to <new-branch>` reports `N added` yet seals onto the OLD branch; the new ref is never created.
- federated checkouts then abort every mutation with `src refspec <new> does not match any`.

This invalidates the SS12 re-home, re-adopt, and center stories for any established checkout; only fresh clones take the working path. Fix: key convergence on the branch the store checkout has, re-materializing (or worktree-switching) when the configured name moved.