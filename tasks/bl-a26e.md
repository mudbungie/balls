+++
title = "Set RELEASE_PLZ_TOKEN so release-PR CI actually runs (today: bot-authored PR, CI held for approval, ends 'failure' with zero jobs)"
created = 1794985973
updated = 1795147288
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["release"]
+++
Cause: release-plz.yml opens the release PR with `secrets.RELEASE_PLZ_TOKEN || secrets.GITHUB_TOKEN`; the secret is unset, so the PR is authored by github-actions[bot] and GitHub holds its pull_request CI run for approval. Nobody approves; it concludes failure with no jobs (seen on PR #443, v0.5.14, 2026-11-18, and the six release-PR runs before it).

Fix (needs the repo owner — a PAT cannot be minted by an agent): fine-grained PAT scoped to this repo, contents + pull-requests write; store as Actions secret RELEASE_PLZ_TOKEN (`gh secret set RELEASE_PLZ_TOKEN`).

Done when: the next release PR is authored by the owner and its CI run executes jobs and goes green. Note the token expires; on lapse the workflow silently falls back to the bot.

---

2026-11-19: RELEASE_PLZ_TOKEN is set (secret listed, updated 2026-11-19T16:49:51Z). Unverified: main has no commits past v0.5.14, so no release PR exists to observe. Verify on the next release PR — author should be the owner, and its pull_request CI run should have jobs and go green — then close.
