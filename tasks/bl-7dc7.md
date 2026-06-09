+++
title = "Docs §6: state the invocation-tree cap is a footgun-guard, not a security boundary"
created = 1781031758
updated = 1781031758
parent = "bl-72a8"
priority = 4
tags = ["docs"]
+++
bl-2d6d security review INFO finding. BALLS_PLUGIN_DEPTH rides the child process environment (src/plugin.rs) and is read back at the binary edge (src/main.rs); a plugin fully controls the environment of any bl it spawns and can reset the odometer to 0, defeating the cap. That is ACCEPTED — a plugin is already arbitrary local code, and the cap (§6 "the runaway backstop") exists to stop accidental cascades, not malicious ones. But the spec never states the inverse: NO future control may be built on the cap as if it were a sandbox (cf. the depth-cap abort now guarding plugin chains incl. security-relevant ones like a forge gate — the abort is reliability, not containment).

DELIVERABLE: amend the §6 cap paragraph in docs/architecture.md with one or two sentences stating the cap is a footgun-guard (env-resettable by a child, not a trust boundary) and that controls must not rely on it. Doc-only, no behavior change. The spec is FROZEN: keep it a non-normative clarification consistent with the bl-7110 resolution, and follow the repo convention for spec edits (worktree + claim/close).