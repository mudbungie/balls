+++
title = "Rewrite the github forge plugin to the subtask model: claim.post mints the review gate, sync resolves it on merge; no close.pre delivery variant"
created = 1781043825
updated = 1781059138
claimant = "Frontages"
priority = 3
tags = ["plugin"]

[[blockers]]
id = "bl-7bfe"
on = "claim"
+++
Repo: /home/mark/dev/balls-github-plugin (cross-repo: ball here, fix there). The current
plugin is the LEGACY-era build (reads `status` fields); no consumers, free to rewrite.

Target model (per bl-7bfe, the spec-side design ball this depended on — closed/delivered):
- claim.post: mint the review gate child — `bl create ... --subtask-of <id>` (bl-788e: the
  one-word parent + close-gate; this plugin is the first programmatic consumer) carrying a
  plugin-namespaced preserved key for the join. Skip minting when the claimed task IS one of
  the plugin's own gate children (no gates-for-gates).
- sync: for each open gate child of the plugin's, check its parent's PR (join: work/<id>
  branch -> PR, or the preserved key); merged => close the gate child. The abandon path's
  manual unlink stays user-side (skill doc).
- rollback claim.post: delete the just-minted gate child.
- NO close.pre hook at all: submission is git-native (worker pushes work/<id>, opens the PR,
  [bl-id] in the PR title so the core delivery's bl-430e tag-scan recognizes the squash-merge
  as delivered and skips the local squash).

INTERPLAY (bl-c231): the already-delivered guard is content-containment (merge-tree no-op),
NOT commit-presence, precisely so this plugin's squash-merge flow — work/<id> never
ancestry-merged — does not false-abort at close. If bl-c231 lands first, nothing here
changes; if this lands first, bl-c231's tests must cover the squash-merge case.

Greenfield port concerns: subprocess contract argv `<op> <phase>`, §7 wire (binding +
previous_state/commit pair), §14 scratch only if the PR number can't be re-derived from the
branch name.