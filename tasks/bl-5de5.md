+++
title = "Build the bl-workhours wrapper (retire the shim)"
created = 1783450180
updated = 1783450180
parent = "bl-c103"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-workhours"]

[[blockers]]
id = "bl-8b98"
on = "claim"
+++
Build the PATH wrapper in ~/dev/bl-workhours (separate repo; git-initialized, design landed bb79e01). CROSS-REPO: ball tracked in the balls center store, code lives in ~/dev/bl-workhours.

--needs bl-8b98 (the core BALLS_CLOCK seam): the wrapper's consistency guarantee can't be verified until core honors BALLS_CLOCK.

SCOPE (design §3-§7, §9):
- The affine map: display_tod = w0 + (real_tod/86400)*(w1-w0), on the real date. One function, every bl invocation, every day -- no verb/work-hours/weekday branch. Monotonic + date-stable (git history order preserved).
- Set BOTH seams to the same instant per invocation: GIT_AUTHOR_DATE + GIT_COMMITTER_DATE (ISO) and BALLS_CLOCK (unix seconds).
- exec real bl at /home/mark/.local/bin/bl by ABSOLUTE path (so balls' own  to .local/bin never disturbs the wrapper).
- Config (policy lives here, not in balls): window_start/window_end (the persona), timezone (default America/Los_Angeles). Env or a small ~/.config file; env is simplest/dependency-free. Pick the default window with the owner.
- FAIL-OPEN: any error path exec's real bl unchanged at real time.
- Loud header comment (what/why/when/pointer to docs/design.md) -- the wrapper is invisible to task-remote   git@github.com:mudbungie/balls.git      origin
task-branch   balls/tasks                             landing
log-level     info                                    landing
claim.post    bl-chore, bl-delivery, bl-tracker       landing
close.post    bl-delivery, github-issues, bl-tracker  landing
close.pre     bl-delivery                             landing
create.post   github-issues, bl-tracker               landing
install.pre   bl-tracker                              landing
prime.post    bl-delivery                             landing
prime.pre     bl-tracker                              landing
show          bl-delivery                             landing
sync.post     github-issues                           landing
sync.pre      bl-tracker                              landing
unclaim.post  bl-delivery, bl-tracker                 landing
update.post   github-issues, bl-tracker               landing

xdg      /home/mark/.config/balls/config.toml
landing  /home/mark/.local/state/balls/clones/%2Fhome%2Fmark%2Fdev%2Fballs/config
store    /home/mark/.local/state/balls/clones/%2Fhome%2Fmark%2Fdev%2Fballs/tasks, so the comment is the only on-box trace.
- v1 weekends: no weekday branch -- map every day identically on its own date (design §6, disposition 1).
- MIGRATION:  shadows bl on PATH and RETIRES the old ~/.cargo/bin/bl shim in the same change (no coexistence).
- Repo standards (AGENTS.md): Makefile (fill the stubs), README (present), bl init if cross-tracking into the center, precommit hooks enforcing 300-line cap + 100% coverage.

Design: ~/dev/bl-workhours/docs/design.md (the living artifact -- edit it like code as the build attacks it).