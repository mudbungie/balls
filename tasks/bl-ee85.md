+++
title = "bl close bypasses the pre-commit gate — clean delivery depends on remembering to run it by hand"
created = 1780984562
updated = 1781034918
claimant = "Beneficent"
priority = 2
tags = ["bug"]
+++
PROBLEM: 'bl close' squashes the work/<id> diff straight onto main; it does not run the repo's pre-commit gate (clippy + line-length + tests + 100% coverage). So whether a delivery is green depends entirely on the agent remembering to run the full gate manually BEFORE close. The safe path is not the default path — it is easy to ship broken or under-covered code through close while every local signal says OK. (Compounded by the staging trap: the hook reads the worktree, close reads the squash.)

This is a footgun, not just ergonomics: the lifecycle's delivery step has no quality barrier of its own.

FIX OPTIONS: (a) wire a default 'close.pre' gate plugin that runs the project's check command and aborts close on failure (severable — projects without a gate opt out via config); or (b) a 'bl close --check' that runs the gate first; or at minimum (c) document loudly that close is gateless and the agent owns the gate. Option (a) makes the safe path the default and keeps policy in the capability, not the core.

Observed during bl-1b49 E2E demo; relates to bl-07d6 (close/seal atomicity).