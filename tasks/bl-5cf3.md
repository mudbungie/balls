+++
title = "Standards-compliance review (clippy/semver/deps)"
created = 1780970015
updated = 1780970577
claimant = "Grainy"
parent = "bl-72a8"
priority = 1
tags = ["review"]
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls/bl-5cf3"
+++
Standards-compliance pass. Run clippy (-D warnings) and resolve; audit for unwrap/expect/panic on non-test paths; verify edition-2021 idioms; check the §5 commit-message protocol conformance in tooling; declare semver/MSRV and stage the pending MAJOR bump (0.4.2 → 1.0.0 at cutover); license headers; dependency footprint (each dep earns its keep — demarcation over removal, correct-substitute test before cutting).

Deliverable: a compliance checklist (pass/fail) + fixes.