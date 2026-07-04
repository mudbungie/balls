+++
title = "Implement query surface: --claimant predicate + claim-age render + discoverability (converged bl-8ab5 design)"
created = 1783194066
updated = 1783194066
priority = 1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["core"]

[[blockers]]
id = "bl-7858"
on = "claim"
+++
Implements docs/design/bl-8ab5-query-surface.md (CONVERGED 2026-07-04). The convergence principle, maintainer verbatim: 'a user _can_ break into the tasks branch and start poking around, but they shouldn't have to to do their basic job. A user having to bust open the tasks branch probably implies a failure of the ergonomic surface.' Basic-job queries get one-command surface answers; the store stays open for archaeology and machines.

DELIVERABLES.
(1) `bl list --claimant NAME` — exact-match compose-AND predicate over the stored claimant field, uniform over live and reconstructed dead rows like every filter (so `bl list -s closed --claimant X` = 'what did X deliver' falls out free). This completes the predicate surface: flags = schema axes (status, tags, date-window, text, claimant), COMPLETE at claimant.
(2) Claim-age, derived at render, NOTHING stored: human list claimed rows render `@Name (3h)` (age attached to the claimant); human show renders `claimed <ISO> (<age> ago)`. Derivation = timestamp of the newest commit touching tasks/<id>.md whose trailer is 'bl-op: claim' (git log -1 --grep='^bl-op: claim$' — newest-wins resolves unclaim/reclaim to the current claim). Live claimed rows only; bedrock --json untouched (stored frontmatter only, per §3 bl-d074).
(3) Discoverability: `bl help list` must document the positional NEEDLE; the skill guide carries the blessed jq idioms and states the one-row-per-ball render contract.

BOUNDARY RULES TO HOLD (from the converged doc — do not relitigate): filters read stored frontmatter ONLY — a derived-fact flag like --stale-over is refused on principle, thresholds are plugin policy (bl-1e98); derived facts are human-render columns, never bedrock fields; no --count, no --sort, no --where, no new verb, no cache, no index.

SPIKE: tag bl-8ab5-spike (stash-shaped commit; git stash apply bl-8ab5-spike) holds the original claimant's unverified partial implementation — REFERENCE ONLY, it predates the bl-7858 decomposition refactor of src/reads.

Gated on bl-7858 (claim): both churn src/reads; serialize behind the refactor.