+++
title = "Amend §11/§15: worktree path computed-not-stored — delete delivery-worktree field; delivery compute-and-prints + read-op dispatch; plugins may run on read ops"
created = 1781030304
updated = 1781030321
claimant = "Sultans"
priority = 1
tags = ["design"]
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls/bl-0af4"
+++
Supersedes the 2026-06-07 bl-934a(1) decision (stage delivery-worktree as a frontmatter field, read via show --json).

WHY the reversal:
- The worktree path is a pure function of (binding, id, claimant) AND already a local git fact (git worktree list shows work/<id> -> path). The staged field was a THIRD copy of something git already owns. SSOT: don't store what you can compute.
- It is machine-LOCAL (XDG_STATE_HOME + invocation_path). The default delivery+tracker combo seals+PUSHES the store, so the field replicated a local path to every clone — wrong for all readers but the one claimant on the claiming machine, who could compute it locally anyway. Local state must not ride shared/remote state.
- bl-934a(1)'s justification was 'stdout is interleaved, not machine-parseable'. False under the existing discipline: tracker writes only stderr; delivery is the sole stdout writer on claim. Keep stdout = the verb's single product (like create prints the id) and the print IS deterministic — no field needed.

THE MODEL:
- Don't write the path anywhere. Delivery computes/reads-from-git and PRINTS it on claim.post (have it) and prime.post (add to the existing claimed-id loop). Those are the two moments you transition into a worktree.
- bl show surfaces it too, in the HUMAN render only (never --json, the lossless store mirror). Core structurally never opens the project repo (§11), so show must DISPATCH to delivery to compute it — READ OPS MAY TRIGGER PLUGINS. Never a deliberate line; documenting the allowance.
- Delete the delivery-worktree key + its claim.pre stage / unclaim.pre clear.

Deliverable: amendment to docs/architecture.md §11 + a superseding §15 entry, with consistency edits to §6 hooks table, §9 claim/unclaim prose, the claim.post line. Code + stale 'bl skill' line are follow-ups.