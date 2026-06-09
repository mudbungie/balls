+++
title = "Release & cutover readiness (1.0 bump)"
created = 1780970020
updated = 1780970536
parent = "bl-72a8"
priority = 1
tags = ["review"]

[[blockers]]
id = "bl-1a66"
on = "claim"

[[blockers]]
id = "bl-0a81"
on = "claim"

[[blockers]]
id = "bl-ff89"
on = "claim"

[[blockers]]
id = "bl-e582"
on = "claim"

[[blockers]]
id = "bl-2d6d"
on = "claim"

[[blockers]]
id = "bl-5cf3"
on = "claim"

[[blockers]]
id = "bl-ae0b"
on = "claim"

[[blockers]]
id = "bl-b156"
on = "claim"

[[blockers]]
id = "bl-9369"
on = "claim"
+++
Gate the milestone: confirm the system is releasable as the major version. Bump 0.4.2 → 1.0.0 (the §17 cutover major bump); finalize the CHANGELOG via release-plz; verify `make install` is reproducible from a clean main worktree and the installed bl self-hosts; re-verify the §17 migration's 4 known holes stay closed (branch collision, epic edges, gh dup, claimed-guard) — cutover already executed (bl-0802) so this is confirmation, not first run; confirm the release-plz marker/tag flow (watch the [minor]/[major] self-match and merge-no-trigger gotchas).

Deliverable: a green release checklist + the version bump landed.