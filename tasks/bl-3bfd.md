+++
title = "Subtract unpopulated §7 wire field field_changes/FieldChange (P1)"
created = 1780973876
updated = 1781031781
claimant = "Sickle"
parent = "bl-72a8"
priority = 1
tags = ["review"]
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls/bl-3bfd"
+++
Subtract the §7 wire field `Command.field_changes` + the `FieldChange` type (src/wire.rs): core NEVER populates it (always Vec::new()) because the op's single source of truth for the diff is the worktree, not a wire-carried changeset. It is a second representation of one fact — the SSOT smell. Surfaced by the minimalism review (bl-004c, P1); cross-referenced by the arch review (bl-1a66).

This touches a SPEC-FROZEN contract (§7 plugin wire payloads), so it is design-significant. FIRST add a §15 revision-log entry in docs/architecture.md justifying the removal and confirm no plugin (tracker, bl-delivery, github-issues) reads the field; THEN remove the field + type + serialization. If a future plugin genuinely needs a core-computed changeset, that is a separate design.

Deliverable: §15 amendment + code removal, gate green. If the maintainer decides to KEEP it (a planned consumer), close with a one-line rationale instead.