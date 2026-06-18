+++
title = "Design doc: bl-chore guarded-mint primitive (docs/design/bl-chore.md)"
created = 1781809838
updated = 1781809838
parent = "bl-6ee9"
priority = 1
+++
Design task for `bl-chore`. **Deliverable is a tracked living document: `docs/design/bl-chore.md`** — edited like code, not grown in this task description. This task records the work; the file is the artifact.

The design doc must converge by attack (don't resolve on first draft). It should pin down:

1. **The guarded-mint-at-claim primitive.** State the framing: `bl-chore` is the create-side building block; forge/build plugins = `bl-chore` (create) + their own resolution. Show how forge's §11 hand-rolled mint would re-express on top (or at minimum, that the guards have ONE home here).
2. **Config schema** — its own territory (`plugins/bl-chore/`), a list of `bl` commands. Decide the exact rendering: how the tag is injected and how `--blocks close` against the claimed task id is applied to a free-form `bl create` line. Resolve the tension named in the epic: "inject into arbitrary shell" vs "declarative child-specs." Recommend, then attack.
3. **The two guards** — tag-skip (always) and epic-skip (knob, default on). Spell out the predicates against the §7 payload, and why epic-skip also yields idempotency.
4. **Dispatch/payload** — what `claim.post` hands the plugin (§6/§7), how it reads the claimed task's id + tags, how it shells `bl create`.
5. **Rollback** — `rollback claim.post` should remove the just-minted children (§11 pattern). Decide whether this is needed or whether epic-skip idempotency makes it moot.
6. **Where it ships** — first-party in the core repo (binary + sample config), opt-in (NOT in the default `[hooks]` schedule). Note the 100% coverage + 300-line constraints apply.
7. **Spec touch points** — which lines of §6 (shipped capabilities, hook list) and §11 (plugin section) the doc-update child must amend. Spec is frozen, so enumerate the edits.

Attack list to answer in the doc: does free-form shell break tag injection? what happens on unclaim→reclaim? does a chore being claimed re-fire (tag-skip must catch it)? does a third-party plugin built on bl-chore that forgets the tag recurse? what if two chores collide on the same title?