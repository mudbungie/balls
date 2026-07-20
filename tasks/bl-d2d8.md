+++
title = "Drive the scalar bl conf surface through the literal CLI"
created = 1784524139
updated = 1784524223
claimant = "Robber"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]

[[blockers]]
id = "bl-71c7"
on = "close"
+++
Integration-test coverage gap (audit bl-covtest, grade C-).

**Target file:** `tests/conf_scalar.rs`

**Exercises:** `bl conf` full dump rows + path lines; `bl conf task-remote` provenance labels across all §12 layers (landing-none/binding/xdg/origin/stealth); `bl conf set task-remote <url>` (clears sentinel) and `none`; set task-branch (+ forbid_landing refusal), log-level (+ invalid refusal), clock-provider; hooks-key wholesale `set`; the usage-error family; conf before prime; conf --as; assert remove-last-name literally drops the key from plugins.toml.

**Assertion:** Each read prints value on stdout + 'conf: <key> from <layer>' on stderr matching the constructed layer; each set lands in the correct file (binding.toml vs landing balls.toml); each refusal exits nonzero with its message; the emptied hooks key is absent from the file.

**Priority:** med. Rules: touch ONLY tests/ (never src/ — keeps the 100% tarpaulin gate green); each .rs file <=300 lines; drive the real bl binary via assert_cmd.