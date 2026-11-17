+++
title = "Bare ids: a ball-id argument that is exactly the shipped scheme's body (4 lower-hex) means bl-<that> — bl show 1234 ≡ bl show bl-1234"
created = 1794896771
updated = 1794896772
claimant = "Without"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-8b4f"
on = "close"
+++
User ask (2026-11-16): omit `bl-` when calling ops dedicated to one ball.

Design: one lexical rule at the CLI boundary, owned by IdScheme (the single home of the prefix fact): an id argument that is exactly `length` chars of `alphabet` is read as `prefix`+arg; anything else passes through untouched. No store lookup, no flag, no config — deterministic, so it works for closed ids and for edge flags (--needs/--blocks/--subtask-of/--parent) alike.

Known hole, accepted: a create/pre plugin that assigns a bare 4-lower-hex id (no prefix) becomes unaddressable from the CLI. Exact-match-wins would need a store read on every id arg and makes meaning depend on store state.