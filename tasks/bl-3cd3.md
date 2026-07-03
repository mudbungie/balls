+++
title = "Probe missing_docs enforcement"
created = 1783111514
updated = 1783111514
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-2f3b"
on = "claim"
+++
From the same 2026-07-03 repo-rectitude review as [[bl-2f3b]], split out because it is a PROBE, not a known-good gate.

Add `[lints.rust] missing_docs = "warn"` to Cargo.toml (the gate's `-D warnings` promotes it to deny, same as pedantic). The doc culture is already strong — §-annotated rustdoc throughout — so enforcement should be cheap insurance that agents keep documenting public items rather than an aspiration.

Decision rule, baked in up front: run the gate once with the lint on. If it is quiet or near-quiet, keep it and doc the stragglers. If it fires on a pile of internal-ish public items, do NOT write filler docs to appease it — either narrow the public surface (pub(crate) what shouldn't be API) or revert the lint and close as rejected. A lint that manufactures boilerplate is worse than no lint (knobs earn their keep).