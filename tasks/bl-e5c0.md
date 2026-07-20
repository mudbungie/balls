+++
title = "Round-trip plain bl import and its collision refusal"
created = 1784524136
updated = 1784524218
claimant = "Robber"
priority = 3
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]
+++
Integration-test coverage gap (audit bl-covtest, grade C-).

**Target file:** `tests/import_roundtrip.rs`

**Exercises:** Pipe `bl show --json` into `bl import --as me` in a second store; import a stream with an in-store collision and an intra-stream duplicate; assert minted-id shape (bl- + 4 hex) and uniqueness across many creates.

**Assertion:** Imported ball is byte-equivalent in show --json (id/timestamps preserved, nothing minted); colliding streams are refused wholesale BEFORE any write, naming the id(s); every minted id matches the IdScheme regex with no duplicates.

**Priority:** high. Rules: touch ONLY tests/ (never src/ — keeps the 100% tarpaulin gate green); each .rs file <=300 lines; drive the real bl binary via assert_cmd.