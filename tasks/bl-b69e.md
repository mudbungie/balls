+++
title = "Delivery reconcile skips any checkout with a local edit: widen bl-22dd's resync to git's two-way checkout merge so unrelated edits are carried forward"
created = 1790732917
updated = 1790732918
claimant = "Proudest"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-37e7"
on = "close"
+++
bl-22dd (src/delivery_reconcile.rs) restores the 'fourth effect' of landing — the checkout on the integration branch — but gates on the checkout sitting EXACTLY at the delivery's parent in index AND working tree. One unrelated local edit anywhere fails the gate, so the checkout stays at the old tree and git status shows the whole delivery as a phantom staged revert (seen 2026-09-25 in ~/userconf on bl-4543, where the checkout is the live deploy). The refuse-on-dirty principle is right; the gate is just coarser than git's own: `git read-tree -m -u <old> <new>` (what git checkout does) updates the files the delivery changed, carries every other local edit forward, and refuses atomically if a delivered file is also edited locally.

Fix: <old> = the newest reflog entry of the integration branch whose tree equals the checkout's current index tree (the index is the true record of where the checkout stands; HEAD^ is wrong after an empty close or two unrefreshed deliveries, and the index tree itself would silently revert staged edits). No match (staged changes) → leave untouched. read-tree refusal → leave untouched, one stderr line naming the command. Never fails the close. Keep: idempotence, every checkout on integration, never a work/<id> checkout.

A prototype of exactly this logic shipped as userconf's bin/bl-refresh (userconf bl-8aa3, 830ded9, tests/test_refresh.sh has the 10 cases) — port its cases here, then userconf deletes bl-refresh (two mechanisms for one effect would drift).