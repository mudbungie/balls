+++
title = "gh-plugin: release readiness — metadata + release-plz config (NO publish)"
created = 1780996140
updated = 1780996140
parent = "bl-10a4"
priority = 3
tags = ["gh-plugin"]
+++
(1) Fix README false 'CI runs the same gate' claim (true once CI lands). (2) Add package metadata absent vs balls: repository, rust-version (MSRV floor), readme/keywords/categories. (3) Add release-plz.toml + CHANGELOG.md + release-plz workflow mirroring balls (semver_check=false, [minor]/[major] regexes, balls:-skip) OR document an explicit install-from-source-only decision. DO NOT trigger any crate publication.