+++
title = "amend adversary docs/design.md §5 to reflect completion-gate.md resolutions (doc debt from bl-c005)"
created = 1782590476
updated = 1782590476
priority = 3
tags = ["adversary", "docs"]
+++
Doc-coherence debt from bl-c005. `docs/completion-gate.md` §9 states design.md "should be amended to point here rather than duplicate", but the amendment wasn't made — `docs/design.md` §5 still reads as fully open.

Amend `docs/design.md`:
- §5 "Model / effort / prompt" (the review-rubric question): mark RESOLVED by completion-gate.md — the gate is rubric-agnostic, the rubric is OWNER CONFIG (completion vs quality), not a fixed review prompt baked into the plugin.
- Where §5 questions are advanced by the two holes (Hole 1 deterministic→pre-commit hook; Hole 2 criterion→intent), cross-reference completion-gate.md rather than restating.
- Keep design.md as the gate-MECHANICS single source of truth; completion-gate.md is the reframe/ecosystem. No duplication.

Deliverable: edited `docs/design.md`.

CROSS-REPO BALL: delivers to ~/dev/balls-adversary — its origin is the balls center, so this ball shows in the shared list, but the worktree must be off the ADVERSARY repo's main. Claim it FROM ~/dev/balls-adversary (and push the code with `git push gh main`, not origin). This ball is itself an instance of the cross-repo-claim hazard under discussion.