+++
title = "Delivery update-ref needs compare-and-swap: concurrent closes silently drop a squash"
created = 1784525520
updated = 1784525520
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]

[[blockers]]
id = "bl-860c"
on = "claim"
+++
CONFIRMED by tests/close_race.rs (bl-860c): delivery_repo_acts.rs::deliver is a non-atomic check-then-act — parent = rev-parse(integration) (l.125), commit-tree -p parent (l.126), then update-ref refs/heads/<int> <commit> (l.133) with NO old-value argument. Two closes in one checkout: A reads main0, B delivers main0->B, A unconditionally overwrites main with a commit parented on main0 — B squash silently dropped (reflog-only, unreported). Shared-checkout agent pools are a documented deployment (close never pushes the code remote; coordination is claim occupancy only), so this is silent data loss in a sane topology.

FIX: pass the pre-read parent as update-ref old-value CAS: git update-ref -m <subj> refs/heads/<int> <new> <parent>. A moved main then rejects the write -> map to the existing loud pre-seal abort (task stays claimed, worktree up, message says main moved, re-run bl close; the retry re-folds main and converges per §14). Same optimistic-concurrency shape the store push already uses. Also FLIP tests/close_race.rs from pinning the drop to asserting both squashes are ancestors of final main (delete the FINDING framing). Add a src-side unit test covering the CAS-reject arm (tarpaulin ignores tests/; 100% gate). Doc touch: skill/close.md concurrency note.