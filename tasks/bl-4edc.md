+++
title = "pin install's validate-after-seal recovery loop: e2e test + skill note"
created = 1781113185
updated = 1781113192
claimant = "Rebut"
parent = "bl-72a8"
tags = ["spec-divergence"]
+++
Decision record (design dialogue, 2026-06-10): the bl-4c45 ordering — protocol-validation/binding refusal firing AFTER the install copy seals — is INTENDED, not provisional. Grounds: the schedule (shared committed text) and the binding (per-machine resolution) are two facts with two homes; gating the transfer on local binaries would entangle them and make the identical install succeed/refuse per box. Dangling bin/<name> is first-class per SS6 (publish ships a recommendation; seed prunes; dispatch errors cleanly, never executes). Converge-on-retry is the rule (bl-c231) and a bind failure is local, so retry = no-op seal + bind. Validate-before-seal was rejected: it adds a pre-commit read path of the SOURCE schedule and makes a center unadoptable until every binary pre-exists, breaking SS12 onboarding.

The one wedge-shaped loop: an adopted schedule wires a dangling plugin into install's OWN pre/post chain, so the retry aborts at dispatch before reaching bind — install cannot repair install. The escape is the existing invariant that bl conf is plugin-free (the config front door can never be blocked by config damage): conf remove install.<phase> <name> -> bl install --bin <name>=<path> -> conf prepend install.<phase> <name>. Hold that property with a test, not luck.

Deliverable: (1) an e2e test (tests/*.rs, coverage-neutral, built-binary harness per docs/e2e convention) that adopts a schedule wiring a dangling plugin into install.pre, observes the clean abort, walks the conf-remove/bind/conf-prepend recovery, and ends with install's chain healthy and the plugin dispatching; (2) one tight SKILL.md line naming the loop and the recipe, in the existing conf/hooks or install area. While writing the test, ALSO probe the corner where the dangling plugin is referenced ONLY in install.pre (after conf remove it is unreferenced, and --bin on an unreferenced plugin is refused — is that sub-case recoverable in-band?); document what is actually true rather than the hoped-for recipe. No mechanism changes: scrutiny, not surface.