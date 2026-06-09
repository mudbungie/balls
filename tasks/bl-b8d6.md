+++
title = "Design: install --to <center> (publish direction) — the open bl-66e7 leg"
created = 1781033527
updated = 1781033527
parent = "bl-72a8"
priority = 3
tags = ["review"]
+++
bl-f387 wired the standalone `bl install` verb end-to-end through the Engine seal, but only for LOCAL seal targets: --to resolves to the landing or the configured store branch; any other target is refused with an error naming bl-66e7 (src/install_run.rs). bl-66e7 itself is CLOSED (the §6 spec ball, delivered 03fe1ed7), so the open question — sealing an install to a CENTER, the §12 publish direction (landing→center, the consent-gated federated-onboarding path §6 describes) — currently lives only in code comments, untracked.

DESIGN QUESTIONS (converge by dialogue before implementing):
- What is the seal target for a remote center — a tracker-mediated push of a landing-shaped commit? The §8 seal is local-only by §0 (core never talks to a remote; the tracker does). Presumably: seal locally to a center-clone checkout, tracker pushes — which suggests the center must be materialized as a local checkout first (the clone-bundle machinery exists).
- Does the §8 phase order need a fetch-pre before Author (the bl-f387 agent flagged that adopt's install.pre tracker fetch cannot ride the engine chain because staging reads FETCH_HEAD — formalizing fetch-before-Author would pull that leg into the §14 trace too)?
- Severability: if publish-direction install never earns its keep (centers are usually adopted FROM, not published TO, and a center admin can run install locally at the center), the right resolution may be SUBTRACTION — keep the refusal permanent and amend §6 to say so.

Deliverable: a §15-logged decision (wire it, or strike the direction from §6), then implementation or doc amendment accordingly.