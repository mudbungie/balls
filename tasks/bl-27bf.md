+++
title = "Rename tracker -> bl-tracker; declare bl- = first-party convention; diagnose the rename on a config miss"
created = 1781807282
updated = 1781809672
claimant = "Entrust"
tags = ["convention"]
+++
DECISION. balls' two first-party plugins should both carry the bl- prefix. bl-delivery already does; tracker is the lone holdout (a calcified inconsistency — the introducing commit bl-f813 records no rationale). Rename the tracker binary + seed to bl-tracker so first-party naming is consistent, and declare bl- RESERVED to first-party plugins as a FORWARD-ONLY convention (mirrors the §5 commit-trailer reservation: 'bl- is RESERVED to core; plugins prefix with their own name'). Third-party plugins name themselves (e.g. the adversary plugin in ~/dev/balls-adversary).

WHY A RENAME, NOT AN ALIAS / NORMALIZER / FLOOR. A resolution-time tracker<->bl-tracker alias is a PERMANENT DIALECT — exactly what §16 rejects ('footprint-demarcation, not a permanent dialect'). A rewrite-on-prime normalizer is a permanent DEADNAME (can never retire — an offline landing might still carry the old name). A compat-version floor is overkill for one cosmetic rename. Instead: accept a small, clean, self-diagnosing break.

THE BREAK + DIAGNOSTIC. Configs founded by balls <= 0.5.3 reference 'tracker'; the renamed binary is 'bl-tracker', so on an established landing the old name no longer binds (seed-prune is first-founding only; rebind never rewrites committed config; config never syncs, §12). Keep a small STATIC formerly-named map (tracker -> bl-tracker). When a referenced hook name fails to bind and is in the map, emit a targeted message instead of the generic 'plugin referenced but not installed here', e.g.: 'the tracker plugin was renamed bl-tracker. If your config (default from balls <= 0.5.3) references the former, update it (bl conf set ... / re-adopt the seed).'

WHY IT'S SAFE. It's tracker, so the break is cheap: a missing tracker is already non-fatal — prime prunes it and a remote-less box runs local. The affected op proceeds locally-only; fixing the name + a normal sync/prime restores remote sync. No data is touched (a config-name edit, not a data migration).

SEVERABILITY. The formerly-named map is a DIAGNOSTIC, not a translation layer: it never makes 'tracker' work, so removing it later merely downgrades to the generic miss error — it is NOT a permanent dialect (unlike an alias).

DELIVERABLE.
- Binary rename: Cargo.toml [[bin]] name, src/bin/tracker.rs -> src/bin/bl-tracker.rs.
- Seed: default-config/plugins.toml — every 'tracker' schedule entry -> 'bl-tracker' (sync.pre/prime.pre/install.pre/prime.post/claim.post/unclaim.post/close.post/create.post/update.post).
- Diagnostic: the formerly-named map + the friendly warning at the bind-miss surface (src/seed.rs bind_present prune path; the dispatch-miss error in src/plugin.rs).
- A §15 post-freeze decision-log entry in docs/architecture.md recording the convention + the rename + this self-diagnosing-break rationale.
- Tests + 100% coverage maintained; 300-line cap respected.

OUT OF SCOPE. The general upgrade-migration ladder / compat floor (convergent normalizers + a refuse-below-floor format version) is a SEPARATE deferred design; its only remaining justification is structural/format migrations, not renames.