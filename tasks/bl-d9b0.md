+++
title = "bl prime dumps raw tracker JSON op-log to stderr on founding (poor first-run UX)"
created = 1782312688
updated = 1782312688
priority = 2
tags = ["ux", "prime"]
+++
PROBLEM
On a clean founding (first `bl prime` in a fresh checkout), the tracker plugin emits its op-log line(s) to stderr as raw JSON, which the user sees as noise. This is a UX wart: a routine, expected condition is surfaced as a machine record.

REPRO (isolated, stealth founding — the cleanest single-line case):
    export XDG_STATE_HOME=$(mktemp -d) XDG_CONFIG_HOME=$(mktemp -d)
    git init -q somerepo && cd somerepo
    bl prime --as me --stealth 1>/dev/null
  prints to stderr:
    {"ts":1782312626,"lvl":"info","src":"tracker","op":"prime","phase":"pre","msg":"tracker: store is stealth (local), not auto-syncing"}

The skill claims `info` ops are quiet ("At the default log_level (info) routine ops are quiet — core narration is debug; what reaches stderr is a plugin speaking (warnings) or a failure."). But here a plugin speaks routine narration at `info` and it leaks to the user as JSON. Either the message should be `debug` (routine, expected — store is stealth on purpose), or the human-facing rendering of plugin op-log lines at info should not be raw JSON.

A secondary symptom: when the tracker hits a git error, EACH line of git stderr is wrapped in its own JSON envelope and dumped, which is even noisier (multi-line git fatal -> N JSON lines). That suggests the rendering, not just the one message level, is the lever.

SCOPE OF THIS BALL (investigation, not the fix)
Diagnose the root cause in the bl source (where tracker emits these lines; how the op-log JSON reaches stderr vs the human renderer; the log_level gating). Decide the right lever: demote routine lines to debug, OR render plugin op-log at info as terse human text instead of JSON, OR both. Attack it (what about genuine warnings/errors that SHOULD reach the user — they must still be legible). Then FILE A SEPARATE IMPLEMENTATION BALL with the concrete, minimal change and close this one. The deliverable of this ball is that implementation ball, not code.