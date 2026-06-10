+++
title = "bl-delivery prime.post rollback exits 1 with No such file or directory on every aborted prime"
created = 1781067250
updated = 1781067250
parent = "bl-72a8"
priority = 3
tags = ["bug"]
+++
Any prime abort (e.g. a later prime.post plugin failing) re-invokes bl-delivery rollback, which exits 1 with `bl-delivery: No such file or directory (os error 2)` and pollutes the unified log with `plugin bl-delivery rollback failed ... may not be unwound` - on every aborted prime, reproducibly.

Core best-effort containment masks it (the op unwinds correctly, claims and worktrees survive, re-prime converges), but SS14 expects sync/prime refreshers to DECLINE with a clean no-op rollback ("an idempotent cache refresh, a re-materialized worktree, declines - undoing it would be the wrong state"). Find what path it tries to read and make the rollback a clean no-op.