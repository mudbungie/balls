+++
title = "checkout-scoped seals carry no SS5 trailers at all"
created = 1781067179
updated = 1781067827
claimant = "Rebut"
parent = "bl-72a8"
priority = 2
tags = ["spec-divergence"]
+++
SS5: "balls always writes bl-protocol, bl-op, bl-actor; bl-id on every per-task op ... absent on the checkout-scoped ops" - i.e. the other three trailers are still present on conf/install/prime commits.

Actual: conf landing commits (`balls: conf set ...`), install commits, and the prime seeds (`balls: found`, `balls: found store`) carry NO trailer block whatsoever - `git interpret-trailers --parse` returns nothing - so op and actor provenance is unrecoverable from landing history. Deliberate in source: install_run.rs comments "no SS5 trailers, install carries none of the task-shaped fields (SS8)" - but SS8 exempts the task-shaped WIRE fields, not the commit protocol.

Resolve: write the three trailers on checkout-scoped seals, or revise SS5 with a SS15 entry. The per-task ops are fully conformant (verified across all five verbs).