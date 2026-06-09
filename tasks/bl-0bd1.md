+++
title = "gh-plugin: release readiness — metadata + release-plz config (NO publish)"
created = 1780996140
updated = 1780997161
claimant = "Scab"
parent = "bl-10a4"
priority = 3
tags = ["gh-plugin"]
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls-github-plugin/bl-0bd1"
+++
(1) Fix README false 'CI runs the same gate' claim (true once CI lands). (2) Add package metadata absent vs balls: repository, rust-version (MSRV floor), readme/keywords/categories. (3) Add release-plz.toml + CHANGELOG.md + release-plz workflow mirroring balls (semver_check=false, [minor]/[major] regexes, balls:-skip) OR document an explicit install-from-source-only decision. DO NOT trigger any crate publication.