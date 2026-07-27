+++
title = "A failed store seal leaves the change worktree committed, so the abort path cannot re-derive the ball id: 'found 0' + FAILED ROLLBACK"
created = 1785027730
updated = 1785124418
claimant = "marrow"
parent = "bl-ea55"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-df0e"
on = "close"
+++
MODE 2 of bl-cdec, root-caused.

Chain: close.pre lands the squash on main (BINDING, correctly stands). Core then seals: Git::seal (src/git.rs) runs `add -A` + `commit` in the change worktree, THEN `merge --ff-only` on the store checkout. Under concurrency the ff loses to a sibling's seal. seal() resets the CHECKOUT (bl-07d6) and returns Err — but nothing resets the CHANGE WORKTREE, which is now committed and CLEAN, and trace.seal is None because seal() never returned Ok.

So the engine unwinds as a PRE-abort: rollback wires carry no metadata (no seal record), and bl-delivery's binary edge calls delivery::resolve_id, whose fallback scans the change worktree for the single changed tasks/<id>.md — and finds zero. 'expected exactly one changed task file, found 0' -> the rollback exits non-zero -> 'plugin bl-delivery rollback failed ... its close.pre side effects may not be unwound'. The operator is told to distrust a state that is fine: the squash is on main, the ball is still open, and the retried close converges (Standing::Settled skips the squash). §7 already names this hole for the post-abort case (bl-430e: 'the post-abort change worktree is clean, so a pre-phase rollback starved of the trailer has no staged task file left to re-derive its id from') and fixed it by putting metadata on the rollback wire. The failed seal is the THIRD state that fix did not anticipate: committed, not integrated, no seal record. The same hole fires on the bl-cf93 narration abort (worktree clean because the tree converged).

Fix (the reframe): identity is an OP INPUT, not a scratch artifact. Put the ball id on the pre wire (wire::Command, op-constant — core knows it: the verb names it, or create mints it) and delete resolve_id's changed-file fallback plus delivery_repo::changed_task_paths. No plugin then re-derives WHICH ball an op is about from mutable staging state, and pre/failed-seal/post all carry the same id.

Rejected alternative: special-case rolling_back with an empty changed set into a silent no-op. That is a branch on a symptom; the missing reframe is that the id was never scratch state.