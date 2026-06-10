+++
title = "stdout single-writer is a default-schedule guarantee, not a discipline; reserve the enveloped-stdout seam"
created = 1781057312
updated = 1781057370
claimant = "Boldface"
priority = 3
tags = ["design", "doc"]
+++
WHY: §0 promises principles "enforced structurally, not by discipline", but §11 defends the
machine-parseable claim stdout with "the discipline forbids that" — a contradiction the
design should own instead of hide. Ergonomics won, deliberately: plugin stdout as the verb's
forwarded product is necessary and stays.

REWORD (§6/§11, doc only): the single-writer stdout property is a DEFAULT-SCHEDULE guarantee,
not a protocol invariant — shipped plugins write diagnostics to stderr, so path=$(bl claim ...)
is reliable as shipped; third-party plugin stdout is forwarded verbatim and unguaranteed.

RESERVE THE SEAM (build nothing — no consumer yet): the future machine channel is enveloping
plugin stdout per-line exactly as stderr already is — the unified log record {ts, lvl, src,
op, phase, msg} (§1) — behind an opt-in. The format exists; do not invent another. A sentence
in §6 naming this is forward-compatibility enough.

Deliverable: spec §6/§11 wording + §15 entry. Doc only.