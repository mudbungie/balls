+++
title = "Refactor pass: subtract dead code, dedup, decompose 240-298-line files to 150-200"
created = 1781588963
updated = 1781588977

[[blockers]]
id = "bl-deac"
on = "close"
+++
Proactive tightening pass (no file is over the 300 cap). Audit found fat concentrated in ~12 files in the 240-298 band where one cohesive sub-unit was never lifted to a sibling. Three tiers: (0) delete dead code, (1) collapse drift to single-source, (2) lift sub-units to siblings + re-export so each file lands 150-200. Each child is its own worktree; close gates on full coverage + 300-line hook. Run serially to avoid main-drift / lib.rs merge races.