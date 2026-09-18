+++
title = "bl-3616: close Q3/Q4/Q5 — shared store is a founded checkout operated via bl -C; drift detected per op in op scope, ball-level at show"
created = 1789701807
updated = 1789701807
claimant = "Inflate"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["design", "docs"]

[[blockers]]
id = "bl-c9c8"
on = "close"
+++
Maintainer 2026-09-17: Q3 'clone it in — we already have the mechanisms to have local checkouts safely stored alongside each other, keeps all operations along the same path, reduces code forks; plumbing under the existence of other controls is how bugs get in.' Q4 agreed. Q5 'drift should be detected, though not necessarily reconciled, at every op, in the scope of the op; list gets everything; ball-level drift shown at bl show.' Amend docs/design/bl-3616-store-tiers.md; surface the residue Q3 creates (nested-op non-publication is process-scoped, must become store-scoped) and the Q5 caveat (behind-drift is only as fresh as the last fetch; no per-op round-trip).