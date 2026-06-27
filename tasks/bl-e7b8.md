+++
title = "terminal plugin op-log lines lost their severity marker after bl-2013 (warning reads same as routine narration)"
created = 1782586050
updated = 1782586050
priority = 3
tags = ["ux", "log"]
+++
CONTEXT
Follow-up to bl-2013 (commit 18e10a7a). To stop `bl prime` dumping raw JSON, the terminal echo in `Log::record` (src/log.rs ~124-133) now renders a plugin line (`src != "core"`) as bare `msg` text. The investigation explicitly left "optionally a one-char severity sigil" as a choice; it was NOT added.

CONSEQUENCE
On the terminal, a routine plugin line and a plugin WARNING now look identical — both are bare text with no severity cue. The three conditional tracker warnings that bl-2013 deliberately kept loud (ephemeral-remote W2 in src/tracker/prime.rs:51, store-elsewhere prime.rs:54, legacy-quarantine src/tracker/remote_ops.rs) reach the user as plain lines indistinguishable from narration. In practice they self-prefix ("tracker: ...") so they stay legible, but the user cannot tell "this is a warning you should act on" from "this is routine" at a glance. The severity that the JSON `"lvl"` field carried is now dropped on the terminal render.

This is mild (the reason it is p3, not folded into bl-2013): the messages are still readable, just not VISUALLY graded. But "make warnings legible" was an explicit bl-2013 must-not-break, and legible-but-not-distinguishable-as-a-warning is a weaker form of it.

SCOPE / OPTIONS (attack before picking; prefer subtraction)
A) Prefix a single severity sigil on the terminal render keyed off `lvl` (e.g. nothing for info, a `!`/`warn:` for the warning level, an `x`/`error:` for error) when `src != "core"`. Cheap, restores the cue. Tension: the Level ladder is deliberately 3 rungs with NO `warn` (src/log.rs:45-57) AND all plugin stderr collapses uniformly to Level::Info at the single relay site (src/plugin.rs ~195-199) — so today a plugin WARNING and a plugin info line are the SAME level; a sigil keyed off `lvl` cannot tell them apart without first giving plugins a way to mark a line as a warning. So A may require B.
B) Decide that the bare-text render IS sufficient (tracker self-prefixes; warnings are exceptional and rare) and CLOSE this as wont-fix, documenting the rationale in docs/architecture.md. Most subtractive — pick this unless a real confusion case is found.
C) If a per-line severity channel for plugins is wanted anyway, that is a bigger change (give plugins a debug/warn channel) that the bl-2013 investigation already weighed and deferred as over-mechanism — do NOT pull it in here without justification.

RECOMMENDATION: lean B (close documented) unless someone hits a real "missed a warning" case; only then do A+the minimal plugin warn-channel. This ball mainly exists so the decision is recorded, not silently dropped.

MUST NOT BREAK
- Do not regress bl-2013: routine founding stays silent at default level; the JSON FILE sink stays byte-for-byte JSON.
- 100% coverage + 300-line cap.

VERIFY (if A is chosen)
A triggered tracker warning (ephemeral-remote: prime with an explicit `--remote` that the durable ladder does not back) shows a visible severity cue on the terminal and is distinguishable from a routine line; routine founding still emits nothing; make test green at 100%.