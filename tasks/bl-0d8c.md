+++
title = "Cross-clone claim contention: the non-ff push is the contention signal"
created = 1784525381
updated = 1784525381
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]
+++
Two clones share a store remote. A claims task X and publishes; stale B claims X before syncing — its claim.post tracker push must reject non-ff, and the documented recovery (bl sync then retry) must surface the occupancy refusal naming A, never a silent overwrite or a both-think-they-own-it state. Also pin what B is left with (local claim? orphaned worktree?) and that sync corrects it. Mirror tests/half_close.rs fixture but for claim. §13 claims "a non-ff IS the contention signal" — proven for close, never for claim, and claim is where two agents actually collide. New file tests/claim_race.rs.