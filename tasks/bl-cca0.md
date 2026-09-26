+++
title = "bl comment never publishes on its own: it dispatches hooks under comment.post, which no seed wires and the tracker's OPS list omits — comments ride the next op's push"
created = 1789712026
updated = 1790393273
claimant = "Proudest"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["tracker"]

[[blockers]]
id = "bl-f113"
on = "close"
+++
Found in the live store-tiers simulation: bl-upstream's component annotations (bl -C <team store> comment …) sealed in the shared checkout and stayed there ('2 seals unpublished') until the next op; a top-level 'bl comment' on any federated checkout — this repo included — behaves the same (bl conf here shows no comment.post row). skill/comment.md says comment 'is sugar over update, nothing more: read the stored body, append, seal', yet its hook key is its own. Two fixes, pick one by dialogue: (a) the seed wires comment.post = [bl-tracker] and the tracker's OPS admits 'comment' (existing landings still need 'bl conf append comment.post bl-tracker'); (b) comment dispatches its hook chain as 'update' (it IS one; the §5 bl-op trailer can stay 'comment' for the journal/bylines), so every plugin wired on update.post — tracker, github-issues, bl-upstream — sees a comment without a second wiring. (b) matches the doc's own sentence and removes a key rather than adding one.

---

DECIDED 2026-09-25 (user): option (a). comment is a top-level update specialization that can be directly addressed — the hook key stays the op token. Fix: tracker OPS admits 'comment'; seed wires comment.post = [bl-tracker]; existing landings get the row by hand. Sibling plugins declare 'comment' in their own protocol ops (bl-upstream here, bl-leak-gate in userconf) — filed separately.
