+++
title = "balls-github-plugin's store checkout diverged from the center store since 2026-07-06: 11 unpublished seals, every publish from that checkout fails — investigate and reconcile"
created = 1790733744
updated = 1790733806
claimant = "Proudest"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-528a"
on = "close"
+++
Found 2026-09-29: `bl -C ~/dev/balls-github-plugin create` fails with the tracker's same-ball E5 (bl-37b1 changed on both sides). Its store checkout ($XDG_STATE_HOME/balls/clones/%2Fhome%2Fmark%2Fdev%2Fballs-github-plugin/tasks) holds 11 local seals dated 2026-07-04..07-06 that never published and is 1476 commits behind the center (git@github.com:mudbungie/balls.git balls/tasks). The subjects look like balls-core ball closes. User delegated the call 2026-09-29: find out what the seals are, whether their content already exists on the center, then publish or discard.

---

Findings. Cause: the center balls/tasks was history-rewritten after 2026-07-06 (every commit from the root's first child on got a new sha; the only content change is a scrubbed author address in two journal lines of bl-37b1 and bl-72a8 in the migrate-legacy seal). The gh-plugin store last fast-forwarded from the center on 07-06 and made no publish attempt until 09-29, so its whole pre-rewrite chain read as local-only: 658 commits ahead / 1497 behind, merge-base = the store's root. The '11 seals' were just the tail of that chain. None of the 658 was sealed uniquely here: git cherry shows 654 patch-identical to center commits; the other 4 (migrate-legacy, greenfield docs pass, two greenfield-impl seals, all early June) differ only by the scrubbed address. The local tip's tree is byte-identical to center commit d9a0500 (07-06 'Update the docs'); every ball it held (bl-5a86, bl-aed7, bl-37b1, bl-72a8) was later closed on the center. Op log: only 2 failures, both 09-29 create.post; changes/ empty. Decision: discard. Tagged the old tip as bl-08ee-backup (e0c91e56) in the store checkout, then reset --hard to the center tip. Verified from the satellite: sync, list, conf all succeed; conf still shows comment.post (bl-leak-gate, bl-tracker); this comment is the first publish through that checkout since the break. Survey of other clones (origin-resolved; report only): balls-adversary has the same stale pre-rewrite chain (568 ahead / 1497 behind, same 4 scrub-only patches) and will hit the same E5 on its next write — same fix applies. One usability-harness cache clone of litany is 3 ahead / 889 behind with 3 unique patches (not inspected). All other origin-backed stores are level; stores whose repo has no origin, or whose origin has no balls/tasks, were not checked.
