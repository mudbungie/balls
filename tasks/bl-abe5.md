+++
title = "Fix arch-conformance gaps from bl-1a66 review"
created = 1780973916
updated = 1780973923
claimant = "Ciphered"
parent = "bl-72a8"
priority = 1
tags = ["fix"]
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls/bl-abe5"
+++
Corrections for the gaps found in the §0–§17 conformance review (docs/reviews/bl-1a66-arch-conformance.md):

C1 (critical) §6/bl-7110 — recursion cap must ABORT+rollback, not run plugin-free. src/plugin.rs suppressed()/run/rollback. Emit a diagnostic naming the op/plugin chain; let the engine unwind. Delete the re-enable-on-nested-call hatch + its doc.
M1 (major) §3/§11 — --json bedrock must include task.extra (toml::Value→serde_json) so unknown preserved keys (team state:, delivery-worktree) round-trip. src/reads.rs task_json.
M2 (major) §6/§11 — worktree-path consumer surface: delivery plugin prints the path on claim.post stdout; core forwards plugin stdout verbatim (not Stdio::null). src/bin/bl-delivery.rs, src/plugin.rs.
M3 (major) §8/§14 — install must use the engine seal/rollback spine (seal to the LANDING). Wire standalone bl install; route install.pre through the engine with reverse-order rollback. src/lib.rs, src/adopt.rs, src/install.rs.
M4 (major) §4/§6/bl-8540 — [hooks] schedule must layer like other config (XDG overlay + _prepend/_append/_ban). src/hooks.rs + config compose_list.
m1 (minor) §7 — populate post-wire current_state (sealed after-state) or strike it from §7. src/mutate.rs after:None.
m2 (minor) — stale doc-comments asserting retired mechanisms: NN- registry (op.rs/lifecycle.rs), coincide-store (layout.rs), suppress-cap (plugin.rs).