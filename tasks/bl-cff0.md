+++
title = "Plugin binary lifecycle edges: signal death and rebinding"
created = 1784525385
updated = 1784525399
claimant = "Revises"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]

[[blockers]]
id = "bl-a09a"
on = "close"
+++
(1) A plugin KILLED BY SIGNAL mid-hook (kill -9 itself after partial stderr) vs a clean exit 1: the op still aborts, prior plugins roll back in reverse, the op-log error record is sane with no controlled exit code (ExitStatus displays differently for signals; stderr may be unflushed). (2) install --bin ghost=/path/A then --bin ghost=/path/B: the binding now points at B and a dispatch actually RUNS B, not stale A (the routine plugin-upgrade gesture, unit-only today). (3) PATH-only resolution: a plugin neither --bin-given nor beside bl, only on $PATH, binds positively (tier only ever asserted in error text). Extend tests/protocol_edges/ + tests/install_semantics/ (this ball owns both dirs; mind shared main.rs mod blocks + 300-line cap).