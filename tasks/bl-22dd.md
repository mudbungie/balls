+++
title = "Delivery regression: squash moves main via update_ref but never updates the landing checkout (phantom staged diff)"
created = 1781585912
updated = 1781585997
+++
## Summary

`bl close` delivery moves `refs/heads/<main>` with a plumbing ref write (`update_ref`) from outside the worktree that has `<main>` checked out. The ref advances but that checkout's index + working tree are never updated, so in the user's primary checkout every delivered file shows as a phantom **staged** change. Regression: earlier the landing was a porcelain op in the checkout that owns main, which updated the working tree as part of the merge.

## Repro

1. Project repo with `main` checked out in a primary (non-bare) checkout where the user works.
2. `bl claim <id>` -> edit in the work worktree -> `bl close <id>`.
3. In the primary checkout, `git status` shows the whole delivered diff as **staged** (`M` in the index column) while `HEAD` is already the new commit. `git diff <prior-HEAD>` is empty (working tree == pre-delivery base). Manual fix each time: `git restore --staged --worktree .`.

## Root cause

Landing a commit on a branch has four independent effects: (1) write objects, (2) move the ref, (3) append a reflog entry, (4) update the index + working tree of the checkout on that branch. The bare/detached-worktree squash path does 1-3 and **skips 4 for the user's checkout**:

- `src/bare_squash.rs::squash_in_detached_worktree` provisions an ephemeral detached worktree, squashes there (so effect 4 lands on the *throwaway* worktree), then `update_ref(refs/heads/<main>, sha)` on the bare gitdir.
- The user's real landing checkout (a separate non-bare worktree on `<main>`) never receives effect 4 -> index/worktree stay at the old tree -> `git status` reports the delta as staged.

Orthogonal to squash-vs-no-ff and to the blank reflog message; those are incidental. The missing piece is specifically the checkout (effect 4) of the worktree that owns `<main>`. Structurally forced because git refuses to merge/checkout/force-update a branch checked out in another worktree, so the code uses `update_ref` to bypass the guard -- and bypassing the guard also bypasses the working-tree update the guard protects.

## Evidence (live repo after delivery)

- `HEAD` -> `refs/heads/main`; primary checkout on main; `core.bare=false`.
- `refs/heads/main` reflog entries for deliveries have **empty messages** (fingerprint of `update_ref` without `-m` -- plumbing, not porcelain `merge`/`reset`).
- index-vs-HEAD == full delivered diff (staged); working-tree-vs-prior-HEAD == empty (checkout pinned to old base).

## Regression

Maintainer reports effect 4 was handled in the original implementation. Related prior work: the `bl-56f4` note in `bare_squash.rs` ("bare repos with linked `.balls-worktrees/` checkouts are a designed-for layout, but the direct-squash code path silently broke them") -- same area; the worktree-sync appears to have regressed again.

## Fix direction

Deliver effect 4 to the checkout that owns `<main>` -- never to agents' `work/<id>` worktrees (those are on their own branch and must stay isolated).

- Prefer: land as a porcelain op *in* the owning checkout (`git -C <checkout> merge --ff-only <sha>` or `reset --keep <sha>`): moves the ref + updates index/worktree + writes a reflog message + refuses on local edits, in one operation. This is literally the "merge back" the workflow already specifies.
- If the ref must move via `update_ref` from the bare gitdir, follow it by reconciling each non-bare checkout that has `<main>` checked out. NOTE: once the ref has already moved, `reset --keep <sha>` is a no-op (diff vs HEAD is empty); repair needs `reset --hard HEAD` / `restore --source=HEAD --staged --worktree` after a cleanliness check (refuse on dirty, do not clobber).
- Atomicity: a ref+worktree update can't be fully atomic (separate locks; worktree writes are multi-file). Make the atomic ref-flip the commit-point and the worktree-sync an idempotent, self-healing reconcile on the next `bl` invocation, so a crash window is a transient state, not manual cleanup.

## Minor, related (fold in or split)

1. `bl close` removes the work *worktree* but leaves the `work/<id>` *branch ref* (orphaned after a squash; not an ancestor of main). Delete it on successful delivery.
2. The `update_ref` writes a blank reflog message -- pass one (e.g. `bl close <id>: deliver`) so `git reflog main` is auditable.