+++
title = "Enrich the legacy migration fixture for field-projection edges"
created = 1784524141
updated = 1784524157
claimant = "Robber"
priority = 1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]
+++
Integration-test coverage gap (audit bl-covtest, grade C-).

**Target file:** `tests/migration.rs`

**Exercises:** Extend the tests/migration.rs legacy fixture with a closed task, an epic, a deferred task, a dangling parent, and a depends_on chain; run list --legacy and import --legacy.

**Assertion:** Closed task is skipped (absent post-import); epic/deferred synthesize tags; the dangling parent is nulled; depends_on mints the blocker AND the epic-waits-on-children reciprocal edge, all visible in show --json of the imported set.

**Priority:** low. Rules: touch ONLY tests/ (never src/ — keeps the 100% tarpaulin gate green); each .rs file <=300 lines; drive the real bl binary via assert_cmd.