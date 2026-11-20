+++
title = "RELEASE_PLZ_TOKEN lacks write access: release-PR job 403s creating its branch — no release PR can open until the PAT is fixed"
created = 1795157000
updated = 1795159110
claimant = "Without"
priority = 1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["release"]

[[blockers]]
id = "bl-6046"
on = "close"
+++
Since the secret was set (2026-11-19), every push to main fails the 'release-plz PR' job: 'failed to create ref refs/heads/release-plz-… Resource not accessible by personal access token (403)' (runs 44673669659, 44675006723). The PAT needs Contents: read+write and Pull requests: read+write on this repo (owner action). bl-a26e closed with the header-comment fix but its real done-condition is still open and lives here.

Done when: a push to main opens a release PR authored by the owner, and that PR's CI run executes jobs and goes green. Re-run the failed job to test: gh run rerun 44675006723 --failed. Do NOT merge the PR — releasing needs the owner's say-so.