+++
title = "Exercise install's copy semantics, directions, and bind-only mode"
created = 1784524140
updated = 1784524224
claimant = "Robber"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]

[[blockers]]
id = "bl-841e"
on = "close"
+++
Integration-test coverage gap (audit bl-covtest, grade C-).

**Target file:** `tests/install_semantics.rs`

**Exercises:** Directory mirror where source dropped a file; single-file and glob union installs; `--to <store-branch>` publish direction; `--to` invalid-ref refusal; bare `bl install` with a configured upstream and with none; `--bin name=path` bind-only (no path/--from); install --as; install before prime.

**Assertion:** Mirror deletes the extra destination file; unions leave unrelated destination files untouched; the store checkout reflects the publish; refusals name 'pass --from <ref>' / both valid --to targets; bind-only copies nothing but binds; refusal precedes any write when unprimed.

**Priority:** med. Rules: touch ONLY tests/ (never src/ — keeps the 100% tarpaulin gate green); each .rs file <=300 lines; drive the real bl binary via assert_cmd.