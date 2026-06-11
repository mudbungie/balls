+++
title = "Core narration is debug: demote mutating-op lifecycle records below the default threshold"
created = 1781149982
updated = 1781149982
+++
Default UX today: every mutating verb prints 3-7 JSON op-log lines (begin / invoke <plugin> / seal) on stderr at the shipped log_level=info. That is noise, not signal - the terse confirmation already tells the operator what happened, and the seal sha duplicates what git history (the store commit) already records.

The mutate/read split in S4 ('core narrates mutating-op lifecycle at info and read-op narration at debug') is a special case, not a rule. REFRAME: severity classifies the VOICE, not the op kind - core narration = debug (all ops, uniform with reads), a plugin speaking (enveloped stderr) = info, failure = error. Default log_level stays 'info': routine verbs go quiet, plugin warnings (e.g. prime's ephemeral-remote warning) still surface, plugin non-zero exits and core aborts always land.

Rejected alternative: shipping default log_level=error. That hides the plugin stderr envelope (warnings vanish), guts the log file by default, and moves a knob instead of dissolving the inconsistency. Demotion fixes every existing checkout on binary upgrade - no config migration, since founded landings carry log_level="info" already.

Scope: lifecycle.rs begin/seal Info->Debug; lifecycle_diffless.rs begin/done Info->Debug (x2); plugin.rs 'invoke <name>' Info->Debug (envelope line stays Info; error records untouched). architecture.md S4 default-mapping sentence rewritten + S15 revision-log entry (substantive change to the frozen spec). SKILL.md output-streams note updated (the 'silence with --log-level error' ritual is obsolete; default is quiet).