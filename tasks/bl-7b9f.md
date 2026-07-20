+++
title = "Run the lifecycle against a bare project repo, stale-worktree recovery, and a real clock provider"
created = 1784524138
updated = 1784524220
claimant = "Robber"
priority = 3
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]
+++
Integration-test coverage gap (audit bl-covtest, grade C-).

**Target file:** `tests/substrate_edges.rs`

**Exercises:** claim→work→close with the invocation path being a bare git repo; rm -rf a materialized work/<id> dir (stale registration) then re-claim; bind a fake clock-provider binary emitting a fixed instant via `bl conf set clock-provider` and run create/close.

**Assertion:** Bare-repo close delivers the tagged squash exactly as a worktree repo; the re-claim prunes the stale registration and re-materializes instead of erroring; frontmatter created/updated AND the delivered main commit's author/committer dates equal the provider's instant.

**Priority:** high. Rules: touch ONLY tests/ (never src/ — keeps the 100% tarpaulin gate green); each .rs file <=300 lines; drive the real bl binary via assert_cmd.