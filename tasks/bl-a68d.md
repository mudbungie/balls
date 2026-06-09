+++
title = "Remove dead §4 config list-merge (_prepend/_append/_ban) (P2)"
created = 1780973878
updated = 1780973878
parent = "bl-72a8"
priority = 1
tags = ["review"]

[[blockers]]
id = "bl-ff89"
on = "claim"
+++
Remove the §4 config list-compose merge — the `_prepend` / `_append` / `_ban` machinery in src/config.rs (~50 lines) plus the tests that only exercise it. CONFIRMED trimmed-scope leftover (maintainer, 2026-06-08): it runs over ZERO list fields today (EffectiveConfig is two scalars; the projection drops the rest), so it is dead generality, not used mechanism. Surfaced by the minimalism review (bl-004c, P2).

Straight removal, not a design call. Drop the merge code, the list-field handling it served, and the orphaned tests; if coverage shifts (a fixture's only consumer was the removed path), tighten or delete the now-dead test. GATED behind bl-ff89 (dead-code sweep, in flight on the same file): by the time this is claimable that sweep has landed — reconcile against current main, and if bl-ff89 already removed it, just verify and close empty.

Deliverable: list-merge gone, gate green (100% coverage, clippy clean).