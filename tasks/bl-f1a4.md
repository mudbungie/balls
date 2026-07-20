+++
title = "External forge squash-merge then bl close retires without re-delivery"
created = 1784525381
updated = 1784525381
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]
+++
The documented submit/approve flow (skill/close.md, §11 FORGE): agent pushes work/<id>, a human squash-merges it into main OUTSIDE bl (broken ancestry, matching content), then bl close must detect content-containment (Standing::Settled via merge-tree), mint NO second [bl-id] squash, and archive the task. Currently only unit-tested against a fake repo — never real git squash-merge semantics through the real binary. A regression double-delivers or wrongly refuses every forge-based close. New file tests/forge_squash.rs.