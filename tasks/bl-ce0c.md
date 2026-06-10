+++
title = "skill note: hook-list ordering is yours - irreversibles last, prepend to post phases"
created = 1781077562
updated = 1781078632
claimant = "Rebut"
priority = 3
tags = ["docs"]
+++
Converged 2026-06-10: strict plugin ordering exists NOWHERE but the seeded defaults - delivery last in close.pre and tracker last in every post phase are defaults, never enforced (core reads no plugin semantics, SS0; SS14 calls sort-last a CONTRACT RECOMMENDATION). If someone writes a bad plugin, so it goes - but we warn them.

The sharp edge to warn about: `bl conf append <op>.post <name>` is the natural wiring gesture and it lands the new plugin AFTER tracker by construction (the seeded lists end with tracker). A plugin that fails there fails after the push: cores un-seal resets a commit the remote already has, the next push is rejected non-ff, and recovery is `bl sync` (the seal resurrects from the remote) + retry. Verified working, but surprising. Same shape for close.pre: anything appended after bl-delivery runs after the squash and leans on the un-squash rollback.

Deliverable: a skill-guide (SKILL.md / bl skill) paragraph: when wiring your own plugin, PREPEND to post phases (or `conf set` the full order); only the irreversible belongs last - trackers push, deliverys squash. Optionally a one-line note in SS6 beside the hook-list table.