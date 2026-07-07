+++
title = "Build the bl-workhours clock-provider binary (retire the shim)"
created = 1783450180
updated = 1783464970
parent = "bl-c103"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-workhours"]

[[blockers]]
id = "bl-8b98"
on = "claim"
+++
Build the clock-provider in ~/dev/bl-workhours (separate repo; git-initialized, design committed 7814ecd). CROSS-REPO: ball tracked in the balls center store, code lives in ~/dev/bl-workhours.

--needs bl-8b98 (the core op-instant SSOT + clock_provider ladder): the provider is meaningless until core resolves T from it and threads T everywhere.

It is a PROVIDER BINARY, not a PATH wrapper. Core invokes it at op-start; it prints one unix-seconds integer (the smeared time) and exits 0.

SCOPE (design §5, §8, §9, §11):
- The affine map: display_tod = w0 + (real_tod/86400)*(w1-w0), on the real date. Pure function of wall-clock; one function, no verb/work-hours/weekday branch. Monotonic + date-stable (git history order preserved).
- Protocol: <bin> -> one line unix-seconds i64 on stdout, exit 0. No §7 payload needed (smear is a pure clock function).
- Config (policy lives HERE, not in balls): window_start/window_end (the persona), timezone (default America/Los_Angeles). Env or a small file; env is simplest. Pick the default window with the owner ([18:00,23:30) reproduces the shim; [09:00,17:00) is never-late).
- v1 weekends: no weekday branch -- map every day identically on its own date (design §6, disposition 1).
- MIGRATION (design §9): make install = build + bl install --bin bl-workhours=<path> + set clock_provider=bl-workhours in the landing config + RETIRE the old ~/.cargo/bin/bl shim in the same change (no coexistence). Wiring is config, not PATH; the wrapper's absolute-path exec trick is gone.
- Repo standards (AGENTS.md): Makefile (fill the stubs), README (present), bl init if cross-tracking into the center, precommit hooks enforcing 300-line cap + 100% coverage.

Design: ~/dev/bl-workhours/docs/design.md (the living artifact -- edit it like code as the build attacks it).