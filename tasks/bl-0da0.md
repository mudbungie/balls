+++
title = "Release & cutover readiness (0.5.0 bump)"
created = 1780970020
updated = 1781036112
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

[[blockers]]
id = "bl-004c"
on = "claim"

[[blockers]]
id = "bl-a68d"
on = "claim"

[[blockers]]
id = "bl-f387"
on = "claim"

[[blockers]]
id = "bl-e196"
on = "claim"

[[blockers]]
id = "bl-3bfd"
on = "claim"

[[blockers]]
id = "bl-07d6"
on = "claim"

[[blockers]]
id = "bl-cc85"
on = "claim"

[[blockers]]
id = "bl-4098"
on = "claim"

[[blockers]]
id = "bl-540e"
on = "claim"

[[blockers]]
id = "bl-587f"
on = "claim"

[[blockers]]
id = "bl-33db"
on = "claim"

[[blockers]]
id = "bl-479d"
on = "claim"
+++
Gate the milestone: confirm the system is releasable. Bump 0.4.2 → 0.5.0 — a MINOR, not 1.0 (the design/API isn't frozen for a 1.0 commitment yet, per maintainer). Finalize the CHANGELOG via release-plz; verify `make install` is reproducible from a clean main worktree and the installed bl self-hosts; re-verify the §17 migration's 4 known holes stay closed (branch collision, epic edges, gh dup, claimed-guard) — cutover already executed (bl-0802) so this is confirmation, not first run; confirm the release-plz marker/tag flow (watch the [minor]/[major] self-match and merge-no-trigger gotchas).

Deliverable: a green release checklist + the 0.5.0 version bump landed.