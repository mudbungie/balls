+++
title = "Assert reconcile heals the integration checkout after delivery"
created = 1784524138
updated = 1784524138
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]
+++
Integration-test coverage gap (audit bl-covtest, grade C-).

**Target file:** `tests/delivery/main.rs`

**Exercises:** Extend the existing lifecycle test: after a fresh close AND after a settled-retry close, run `git status --porcelain` and `git diff --cached` in the root integration checkout that was sitting one commit behind.

**Assertion:** Both are empty — no bl-22dd phantom staged diff in either the fresh-squash or Standing::Settled skip path.

**Priority:** med. Rules: touch ONLY tests/ (never src/ — keeps the 100% tarpaulin gate green); each .rs file <=300 lines; drive the real bl binary via assert_cmd.