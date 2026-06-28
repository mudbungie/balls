+++
title = "bl close delivery squash drops the work-branch commit body — main commit is only the ball title, losing all rich context"
created = 1782610051
updated = 1782614388
claimant = "Crony"
priority = 2
tags = ["core"]
+++
Observed in the 0.5.4 sprint, 2026-06-27. Subagents wrote rich multi-line commit bodies (rationale, what-changed, must-not-break) on their `work/<id>` branches, but `bl close`'s delivery squash produced a `main` commit whose message is ONLY the ball TITLE — a single line, no body. The rich context is lost: it survived only in the work-branch reflog, which the settled-branch prune then removes.

This directly undercuts the project rule "write rich commit bodies" (release-plz builds the CHANGELOG from commit subjects+bodies): the v0.5.4 CHANGELOG entries are all terse one-line ball titles, even though every delivery had a detailed body written by its author.

Deliverable: `bl close` should carry the delivered work's message into the `main` delivery commit body — the single authoritative source to decide on:
- preserve the work-branch HEAD commit's full message (subject + body), or
- the concatenation/squash of all work-branch commit messages, or
- honor `-m` as a FULL message (today `-m` is described as just "the commit note").

Pick one source of truth for the delivery message so rich context reaches `git log main` and the changelog. Tag: ux.