+++
title = "Design: query surface — packaged derived reads over the store (filter, search, age, count), no index"
created = 1782968223
updated = 1782969169
claimant = "Paint"
priority = 1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["core"]
+++
Design ball — deliverable is a tracked doc under docs/design/ named for this ball (convention: bl-<id>-<slug>.md), converged by dialogue with the maintainer, not a first draft. The ball records the work; the doc is the artifact.

PROBLEM. The query surface is thin: `bl list` supports only `-s ready|blocked|claimed|closed`, `--all`, `--tag`, `--json`. No claimant filter, no text search over title/body, no counts, no sort control, no claim-age. The sanctioned fallback is bedrock `--json` piped to jq — philosophically consistent (the bedrock projection is the machine contract), but every agent re-derives the same filters every session, and fleet-operational questions ("what is agent X holding? what's been claimed longest? which balls mention the delivery plugin?") have no one-command answer.

MAINTAINER'S STANCE (verbatim, 2026-07-01): "I tend to think that index building is a giant complication. Technically everything is files, and a couple of clever greps and the like could deliver it, if the data is structured right. Is this just packaging command surface to deliver those queries?"

CONSTRAINTS / PHILOSOPHY TO BAKE IN. No index — never store what you can compute; the store IS the data structure (tasks/*.md on the store branch; closed = history). Queries are derived reads: greps over live task files + git-log walks for the closed set and for time-of-event facts (claim time is the claim commit's timestamp — it is NOT a frontmatter field and must not become one). `--json` stays the lossless bedrock mirror of stored frontmatter; derived query OUTPUT must not leak derived fields into bedrock.

QUESTIONS THE DOC MUST SETTLE. (1) Enumerate the queries a fleet actually runs hourly — claimant, claim-age, tag, text-match, counts-by-status — and which are list flags vs jq-is-fine. (2) Flags per query vs one generic predicate surface — where does flag proliferation become the smell? (3) Closed-set queries: what does an O(history) walk cost on a long-lived store, and is that just an accepted price (no cache — see the no-index rule)? (4) New verb vs new flags on `list` (a new verb is a smell; prefer the existing signal). (5) What exactly falls out for the liveness design ball that is gated on this one — claim-age surfacing is that ball's foundation.