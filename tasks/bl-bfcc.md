+++
title = "Implement [source] hints + install dangling-report + conf unbound section (converged bl-5b09)"
created = 1783197560
updated = 1783197560
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["core"]
+++
Implements CONVERGED docs/design/bl-5b09-capability-distribution.md (maintainer dialogue 2026-07-04, bl-f338) + the architecture.md §15 entry. Doctrine: distribution is the package manager's job — balls ships a pointer, not a pipeline. The invariant: nothing implicit, ever — hints are display-only, never parsed, never executed; an explicit -y/--trust surface is DEFERRED pending a provenance design (maintainer: 'Answering provenance is a bigger operation') — do not build any execute-the-hint surface here.

DELIVERABLES.
(1) `[source]` table in plugins.toml: name → free-text acquisition hint (e.g. bl-adversary = "cargo install balls-adversary"). Layered like [hooks] (§4: landing + XDG merge, per-name scalar, innermost wins) — same file-load the dispatch already does; the table already round-trips untouched on install (Hooks::parse ignores non-[hooks] tables). Rendered verbatim through the ordinary stderr/log path with control characters stripped (untrusted display text, same discipline as enveloped plugin stderr).
(2) Hints decorate EXISTING refusal moments only — no new verb, flag, or moment: (a) dispatch unbound-name error appends '— source: <hint> — then bl install to bind'; (b) install validation refusal (does-not-speak-protocol) appends '— source: <hint>' (doubles as the stale-binary upgrade pointer); (c) seed prune stays silent for hintless names but a pruned name WITH a hint gets one stderr line (loudness keyed on hint presence = the org opted in by authoring it).
(3) Honesty fix, hint-independent: `bl install` reports what bind_referenced left dangling (today silently skipped — Summary says 'N added' and the surprise arrives at the next close). One stderr info line per referenced-but-unbound name, with hint if present; re-run converges per §14.
(4) Honesty fix, hint-independent: `bl conf` dump grows an `unbound` section after the hook rows — one row per referenced-but-unbound name with its hint or '(no source given)'; all bound ⇒ section ABSENT (the general path with empty inputs, not a special case). Bound-state derived at read (resolve each referenced name against the registry) — a query, not a field. Hook rows stay unmarked: 'unbound' has one home in the dump.

BOUNDARIES. Core never reads config/plugins/<name>/ (the hint lives in core's own plugins.toml precisely so it is readable when the plugin is NOT there to speak for itself); dispatch inputs unchanged — same refusals, different words; severable: deleting every [source] entry yields bit-identical behavior with terser errors. Exact message strings are drafted in the design doc §2–§3 — follow them.