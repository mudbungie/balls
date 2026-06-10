+++
title = "Delivery fold rigor: strict merge + no-resurrection path invariant"
created = 1781057282
updated = 1781059131
claimant = "Frontages"
parent = "bl-72a8"
priority = 1
tags = ["design"]
+++
WHY: bl-33db resurrected bl-ffaf's delivered deletion via a modify/delete fold resolved
work-side — a wrong tree shipped silently THROUGH the gate. §11 specs only "a conflict
aborts the half-merge"; the fold's resolution policy is unspecified. The store got one-step
rigor (§13: "a non-ff IS the contention signal"); delivery — the seam parallel agents hit
hardest — never did. Same rigor, delivery-shaped:

(a) STRICT FOLD. The fold is git's default merge with NO resolution strategy (-X forbidden);
anything git marks conflicted — modify/delete and rename/delete included — aborts the close.
Git's default already conflicts on modify/delete, so the observed resurrection implies the
implementation resolved it some other way; the spec must forbid that, not just imply it.

(b) NO-RESURRECTION INVARIANT, checked at squash: paths(squash diff vs integration tip) must
be a subset of paths authored on work/<id> since its fork (fold-conflict resolutions are work
commits, so they count). An excess path means the squash carries something the task never
wrote — resurrection or leak — and the close ABORTS naming the path. Pure plumbing (two
diff --name-only set-compares), zero new state, derive-don't-store. Would have caught bl-33db
at close instead of at bl-2546.

Deliverable: spec §11 + §15 entry + delivery plugin code + tests.