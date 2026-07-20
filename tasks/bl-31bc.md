+++
title = "Prove delivery standing, message, and no-op semantics"
created = 1784524138
updated = 1784524221
claimant = "Robber"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]

[[blockers]]
id = "bl-810d"
on = "close"
+++
Integration-test coverage gap (audit bl-covtest, grade C-).

**Target file:** `tests/delivery/standing.rs`

**Exercises:** Standing::Diverged (deliver, then commit more on the surviving work branch, close again + prime); close -m with a multi-commit work branch; close with zero work commits (empty deliverable) and close of a never-claimed epic; `git branch -D work/<id>` then close.

**Assertion:** Diverged close aborts with 'already delivered ... file a new task' and prime's prune preserves the branch; the squash BODY is subject + -m narration + work messages oldest-first; empty deliverable lands a bare tagged no-diff commit; the branch -D discard close succeeds with no delivery commit.

**Priority:** med. Rules: touch ONLY tests/ (never src/ — keeps the 100% tarpaulin gate green); each .rs file <=300 lines; drive the real bl binary via assert_cmd.