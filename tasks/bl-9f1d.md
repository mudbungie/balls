+++
title = "Stale-read close guard: seen-token CAS on task content, homed by context"
created = 1784613730
updated = 1784613737
claimant = "Serener"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-8448"
on = "close"
+++
## The race

An agent claims a task; another agent updates the task file mid-flight; the claimant closes without ever seeing the update, sealing an amended contract blind. Same invariant class as the delivery update-ref CAS (bl-a3bb) one layer up: close must act on the content it believes it acts on. The task file IS the contract close seals; archiving unseen content is a torn write at the semantic layer.

## The invariant (core)

`bl close <id>` refuses iff the task file changed since the closer's own last touch of it (derived from store-branch history — claim counts, so the anchor always exists; no state) AND no matching seen-token is found. The refusal prints the since-claim diff and mints the token itself, so a bare retry passes — worst case anywhere in the system is exactly ONE refusal-with-diff. If yet another edit lands between refusal and retry, it refuses again with the new diff: CAS semantics intact.

## The token

A file named for the ball id, content = the task file's blob sha as displayed. Minted by every `bl show <id>` — safe to mint eagerly because a token can only ever SKIP a refusal whose content was just put on someone's stdout, never cause one. Stray tokens are inert.

**Mint home — the home IS the semantics.** Standing in a `work/<id>` worktree → that worktree's gitdir (per-agent scope). Anywhere else — repo root, subdirectory, non-git dir — → the store clone's own gitdir. Scope-by-home is the proof-of-sight: the worktree rung is what stops a writer's own verify-after-edit `bl show` from acknowledging the edit on the claimant's behalf (the single most likely show between edit and close — a shared home defeats the mechanism in exactly the motivating scenario). The store rung always exists, so core has NO git-or-not branch: gitless (delivery-detached) invocation takes the identical code path. And bl never writes into the userspace `.git` — tokens live only in territory bl owns (XDG store clone, bl-created worktree gitdirs).

**Read:** union of store gitdir + the task's worktree gitdir (computed from the id, never from cwd) + current worktree gitdir if standing in one. Misses are meaningless; hits only skip refusals.

**Cleanup:** worktree tokens die with the teardown close already performs; successful close deletes the token it consumed; `bl prime` sweeps tokens naming absent task files (absence is the closed-record — a dead token is self-identifying debris, consistent with prime pruning settled work/* branches).

## Why this shape (attacked four rounds; each round removed mechanism)

- No `--seen` flag: the sha was only ever a proof that the diff entered the closer's context; refusal-mints-then-bare-retry gives the same proof with zero surface.
- No identity keying: `--as` is an unenforced convention; a gate keyed on it quietly evaporates under name collision. This mechanism uses none — and it must not be STRONGER than identity either, since cooperative occupancy is the constitution (a seen-gate cannot outrank the claim it guards).
- No read-receipt state in the store: reads are events, not derivable facts; the token is a local, loss-safe cursor (deleting it costs one refusal, never correctness). The store branch stays the single source of content truth — "derived, never stored" holds.
- No config knob: the unraced and self-edit flows have zero friction, so there is no constituency for turning it off. Friction lands only on the true positive — which is the feature firing, not friction.
- Severability: the worktree rung is the delivery capability's enrichment. Detach delivery and the mechanism collapses to the store rung with zero code edits.

## Accepted residue

- Agents colocated at the same invocation path share the store-rung scope (one could acknowledge for another). Bounded by the same cooperative trust as claims/unclaim; requires colocation, not just name collision.
- bl-0bd8 (substrate keyed on literal cwd) splits token scope on subdirectory invocation — one harmless extra refusal. This design is one more consumer that benefits when 0bd8 is fixed.
- `bl show > /dev/null` defeats proof-of-sight; irreducible, and the refusal path still shows the diff once.