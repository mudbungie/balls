+++
title = "Test-suite & coverage-honesty audit"
created = 1780970017
updated = 1780970017
parent = "bl-72a8"
priority = 1
tags = ["review"]
+++
Audit test quality beyond the 100% line-coverage gate. Verify coverage is HONEST — known tarpaulin false-negatives (multi-line Some(Struct{…}) tail-return; generic-monomorphization gaps) aren't masking real holes. Confirm integration tests exec the freshly-built binary by path against throwaway /tmp repos. Flakiness review: env-race set_var/remove_var on HOME/PATH and git-SHA same-second coincidence are known-fixed — confirm no regressions. Identify untested paths in federation, §17 migration, and forge-gated delivery.

Deliverable: coverage-honesty findings + tests added for the gaps.