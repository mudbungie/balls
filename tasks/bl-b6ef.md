+++
title = "core op-log lines still print as raw JSON to the terminal on error paths (follow-up to bl-2013)"
created = 1782586026
updated = 1782586026
priority = 3
tags = ["ux", "log"]
+++
CONTEXT
Follow-up to bl-2013 (commit 18e10a7a), which split the terminal echo in `Log::record` (src/log.rs ~124-133): plugin lines (`src != "core"`) now render as their `msg` text, while `src == "core"` lines still `eprint!("{line}")` the raw JSON envelope. That fixed the reported `bl prime` founding noise AND made plugin warnings / multi-line git errors legible.

RESIDUAL
The same "JSON on the terminal" wart still bites on the CORE error path. Reproduce (installed binary, post-bl-2013):
    export XDG_STATE_HOME=$(mktemp -d) XDG_CONFIG_HOME=$(mktemp -d)
    cd $(mktemp -d) && git init -q .
    bl prime --as me --remote /nonexistent/repo.git 2>&1 1>/dev/null
  tail of stderr:
    {"ts":...,"lvl":"error","src":"core","op":"prime","phase":"pre","msg":"plugin bl-tracker aborted the op (exit status: 1)"}
    {"ts":...,"lvl":"error","src":"core","op":"prime","msg":"abort plugin failed during prime, rolled back 0 prior: plugin bl-tracker aborted the op (exit status: 1)"}
    bl: plugin failed during prime, rolled back 0 prior: plugin bl-tracker aborted the op (exit status: 1)
The plugin (`src=tracker`) lines above these now render as clean text — good. But the two `src=core` error records still dump as JSON. Worse, the 2nd JSON line is VERBATIM the human `bl: ...` summary that follows it — pure redundancy. Narrower than the original wart (fires only on real failures, not every founding) but it is the same shape.

SCOPE / OPTIONS (attack before picking)
A) Apply the same human-render to core terminal lines: when echoing to stderr, render `src == "core"` as `msg` (with a severity marker — see the severity-flattening sibling ball) instead of the JSON envelope. Symmetric with the plugin path; the JSON-lines FILE sink stays byte-for-byte JSON (must-not-break).
B) Suppress the core JSON on the TERMINAL entirely and rely on the `bl: ...` summary + the file sink. Most subtractive, but loses the phase-pre "which plugin / exit status" detail (line 1 above) that the `bl:` summary omits — so A is probably right unless that detail is judged noise.
Either way: terminal = human, file = JSON, errors stay loud and legible. Do NOT change the file sink.

MUST NOT BREAK
- The per-clone op-log FILE keeps full JSON-lines records for every event (it is the machine record).
- An actual failure must still surface legibly and the `bl: ...` exit message must remain.
- 100% coverage + 300-line cap; log_tests.rs will need updates.

VERIFY
The repro above shows NO raw JSON on stderr (only readable text + the `bl:` line); the file sink still holds the JSON records; a successful op is unaffected; make test green at 100%.