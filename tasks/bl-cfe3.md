+++
title = "bl install --bin is bimodal and a footgun on a tracked repo — it mirrors the upstream's stale config over the local landing"
created = 1783490048
updated = 1783490883
claimant = "clock-refactor"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["design"]

[[blockers]]
id = "bl-205b"
on = "close"
+++
CONVERGED DIRECTION (maintainer dialogue 2026-07-07): the `--bin` welded binary-binding onto the wrong verb. Binding a clock provider is NOT an install / RCE-import concern — it is a low-velocity local CONF setting; users need just enough access to update it, no special verb.

DEEPER: the bin/<name> indirection (a portable committed NAME + a per-machine local binding + the RCE consent gate) exists for SHARED PLUGIN SCHEDULES — a team commits a schedule of names, each machine binds them to its own binaries, and adopting a schedule can't run code until you locally bind. The clock provider is BOX-LOCAL (§1 "this box only"), cosmetic, and fail-open — none of that rationale applies. So it should NOT be a bound name at all: `clock_provider` should be a directly-set LOCAL value (an absolute path, or a PATH-resolved name) in the per-machine local-trust layer (XDG / the per-clone binding, which never travels on `install`), set by `bl conf`. No install, no bin/<name> symlink. This honors "conf never touches binaries / config-is-RCE" (§4): a box-local value in the NON-TRAVELING local-trust layer carries no adoption-RCE — it is your own machine's setting, not something a fetched config can smuggle in.

IMPLICATION: bl-98ba (the follow-up that made `bl install --bin` bind a clock provider) was solving the WRONG problem — the clock never needed install-binding — and is REVERTABLE once clock_provider is a conf-set local path. It also dissolves the migration pain (one per-machine setting instead of the per-landing bin symlinks used 2026-07-07).

SEPARATE, SMALLER concern that remains: `bl install --bin` is still bimodal for actual PLUGINS (bind-only on a stealth landing, config-MIRROR on a tracked one). Plugins legitimately need the name+binding split (they ARE shared/adopted), so the fix there is narrower — `--bin` with no explicit path and no `--from` = bind-only ALWAYS; config adoption stays opt-in via a named path or `--from`, so "just rebind a plugin binary" never silently mirrors the upstream's stale config.

ORIGINAL FOOTGUN (what triggered this): migrating bl-workhours, `bl install --bin bl-workhours=<path>` on the tracked balls landing fetched github's month-old balls/config and MIRRORED it over the local landing — reverting the tracker->bl-tracker rename, re-wiring the broken github-issues, dropping clock_provider. Recovered via `git reset --hard <pre-install>` in the landing checkout. Config is single-owner and never auto-pushed (§4/§12), so an upstream's balls/config is chronically STALE — any `bl install` reverts real local progress.

DOCS (required when implemented): architecture §4 — clock_provider becomes a directly-set LOCAL value (a path or PATH-resolved name) in the non-traveling local-trust layer, NOT a landing-config name bound via install; state that it never travels on `install`. §6 + §8 — remove "clock_provider is bound by `bl install --bin`"; the `install --bin` bind path is for PLUGINS only (and made bind-only-always when no path/--from is given). § id generation cross-ref — the clock is resolved from a conf value, it is not part of the bin/<name> reassign family. §15 — record: bl-98ba was the wrong layer and is reverted; the `install --bin` bimodality narrowed to bind-only-always for plugins. Also REVERT the bl-98ba doc additions (§4/§6/§8) that describe the clock as install-bound.