+++
title = "Make delivery Git environment unable to bypass the repository hook"
created = 1785727317
updated = 1785727679
claimant = "codex-balls-hook-hardening"
priority = 4
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bug", "security"]

[[blockers]]
id = "bl-a69c"
on = "close"
+++
Found during the Silt/Obsidian incident hardening on 2026-08-01. Balls 0.5.9 delivery launches Git while inheriting the caller environment. A caller can set GIT_CONFIG_COUNT=1, GIT_CONFIG_KEY_0=core.hooksPath, and GIT_CONFIG_VALUE_0=/dev/null; the delivery transaction then omits the repository's pre-commit/reference-transaction safety hook and can advance main without the broker receipt. This violates the stated close invariant that delivery is gated by the repo hook. Deliverable: make every delivery Git subprocess use a deliberately constructed environment that cannot inherit Git configuration injection (including the indexed GIT_CONFIG_KEY_N/GIT_CONFIG_VALUE_N family and other Git configuration/search-path variables); retain only the minimum environment required for correct author identity and Git operation. Add a negative integration test proving a hostile caller environment cannot suppress or replace the configured hook. Preserve intended repository/local configuration behavior and document the exact trust boundary. All repository tests, the source-length gate, and exact 100% union coverage must pass before close.