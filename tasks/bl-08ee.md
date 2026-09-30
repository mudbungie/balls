+++
title = "balls-github-plugin's store checkout diverged from the center store since 2026-07-06: 11 unpublished seals, every publish from that checkout fails — investigate and reconcile"
created = 1790733744
updated = 1790733744
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
Found 2026-09-29: `bl -C ~/dev/balls-github-plugin create` fails with the tracker's same-ball E5 (bl-37b1 changed on both sides). Its store checkout ($XDG_STATE_HOME/balls/clones/%2Fhome%2Fmark%2Fdev%2Fballs-github-plugin/tasks) holds 11 local seals dated 2026-07-04..07-06 that never published and is 1476 commits behind the center (git@github.com:mudbungie/balls.git balls/tasks). The subjects look like balls-core ball closes. User delegated the call 2026-09-29: find out what the seals are, whether their content already exists on the center, then publish or discard.