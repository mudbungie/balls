+++
title = "bl install --bin is bimodal and a footgun on a tracked repo — it mirrors the upstream's stale config over the local landing"
created = 1783490048
updated = 1783490048
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["design"]
+++
SHARP EDGE found migrating bl-workhours (2026-07-07). 'bl install --bin <name>=<path>' READS as 'bind a binary', but bl install's default action is 'MIRROR the whole config/ from the upstream' (path defaults to DEFAULT_PATH=config, a destructive folder-mirror, §6). So the same command has TWO wildly different effects depending on invisible context:
- STEALTH landing (no upstream resolves): bind-only (bl-98ba's mode) — just binds.
- TRACKED landing (upstream resolves): FETCH the upstream's balls/config and MIRROR it over the local landing, reverting all local config divergence (clock_provider, schedule edits, source hints) to the upstream's — which is chronically STALE because config is single-owner and NEVER auto-pushed (§4/§12). 
I ran 'bl install --bin bl-workhours=<path>' to bind a provider and it silently reverted the balls landing's entire [hooks] schedule to github's month-old config (undid the tracker->bl-tracker rename, re-wired a broken github-issues, dropped clock_provider). Recovered by 'git reset --hard <pre-install>' in the landing checkout.

WHY IT'S BAD: bind-only vs config-clobber turns on whether an upstream happens to resolve — invisible to the caller. bl-98ba (make install --bin bind a clock provider) only made bind-only reachable in the stealth case, so it left the footgun armed on every tracked repo.

DIRECTION (subtraction-first, to attack not adopt): '--bin' with NO explicit path and NO --from should be bind-ONLY, ALWAYS. Config adoption becomes opt-IN — you name a path ('bl install config --bin ...') or a source ('bl install --from <ref> --bin ...'). 'Just bind' never silently adopts config, and bind-only stops depending on upstream presence. ATTACK: does this break federated onboarding (adopt a center's schedule + bind its plugins in one shot)? No — that flow already names the source. 
DEEPER SMELL worth attacking before settling: is conflating config-ADOPTION (a consent-gated remote import, §6) and binary-BINDING (a purely local resolve) in ONE verb the root problem? The '--bin' rider blurred a local op onto a remote-adoption op. Maybe binding wants its own narrow surface. Keep OPEN for dialogue (don't resolve on first draft). See also the wiring runbook ~/ops/bl-workhours-wiring.md which documents 'never bl install on a tracked repo — use a direct bin symlink'.