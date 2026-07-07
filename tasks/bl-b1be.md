+++
title = "Dissolve the two bl-bfcc display asymmetries: seed prune note through the op log; rename notice carries the new name's [source] hint"
created = 1783397961
updated = 1783397968
claimant = "Wagered"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
(1) The seed prune note is a bare eprintln inside seed_landing — the one hint surface that bypasses the ordinary stderr/log path: it never lands in the per-clone op log and ignores log-level. Fix: seed_landing surfaces its pruned-hinted notes to the caller and prime emits them through the op Log at info, like install's dangling report (an actionable incompleteness report, not core narration) — same rendered line, now persisted and threshold-gated.

(2) A renamed first-party name that is unbound is SKIPPED with the rename notice and never shows an acquisition pointer even when [source] is authored. Fix: the name→hint stitch falls back to the renamed-to name's hint (the coherent one: the remedy is a conf edit to the NEW name, and the new name's hint says where its binary comes from), and the rename notice appends '— source: <hint>' when present.

Same refusals, same formats, strictly more words when the org authored them; hintless behavior byte-identical. Docs: architecture §12 seed-note sentence (stderr → the op log path). Follow-up to bl-bfcc (converged bl-5b09).