# bl-c3c0 — a lagging clone makes bogus prime worktrees + a "half-closed" task

**CONVERGED 2026-06-28 (maintainer dialogue, Demagogy).** The ball recorded two
failures from the 0.5.4 cross-tracking sprint (`~/dev/balls` +
`~/dev/balls-adversary` on one shared `balls/tasks`). The first design pass wrongly
unified them under "stale store — reconcile before acting"; the maintainer split
them into two unrelated root causes with much smaller fixes:

- **Bogus prime worktrees → delete prime's worktree re-materialization.**
  Worktrees materialize at **claim and nowhere else**; re-priming a lost worktree
  is `unclaim` + `claim`. Pure subtraction — needs no repo identity and **moots
  all sync-ordering analysis** (a prime that never materializes can't make a
  bogus worktree, whatever the store's freshness or hook order).
- **Wrong-repo claim (a companion hazard) → claim guards on the repo's root
  commit.** A ball records its project's root-commit hash at create; `claim`
  rejects when the current repo's root differs, hinting at the right checkout.
  Root-commit identity is canonical across hosts and **needs no remote**. **No
  override / escape-hatch:** a same-root collision *fails open* and grants nothing
  — the attacker would already have to be working from the colliding repo's
  directory, which is the precondition for any claim.
- **The "half-close" is not a defect → pave it, don't prevent it.**
  Merge-landed + task-still-open is the **correct** failure direction (the agent
  stays engaged; nothing is left dangling). Do **not** pre-reconcile (a common,
  well-worn path beats a rare edge case) and do **not** auto-sync (the
  `close → reject → sync → close` loop is already a path agents walk for
  main-fold conflicts). The work is messaging.

## Two reported failures, two different bugs

1. **Bogus prime worktrees.** On a clone behind the shared remote, `bl prime`
   materialized `work/<id>` worktrees off the *wrong repo's* `main` for tasks
   closed on the remote. Cleanup needed `git worktree remove --force` +
   `git branch -D`.
2. **"Half-close."** `bl close` on a lagging clone delivered the squash to `main`
   but the archival push was rejected non-ff; the task stayed `claimed`.

The first pass treated both as "act on a stale store, discover it too late at the
push." That conflated a **worktree-lifecycle** bug (#1, no remote involved) with a
**federation-publish** outcome (#2) — and tried to *prevent* the half-close the
maintainer wants kept. The two fixes below are independent.

## Ground truth

- `prime.post = ["bl-delivery", "bl-tracker"]` (`default-config/plugins.toml:20`).
  `bl-delivery`'s `prime.post` loops `claimed_ids` off the local store and
  re-materializes a worktree per id (`src/bin/bl-delivery.rs:120-126`), then
  prunes settled `work/*` branches (`:128`). On a stale store a closed task still
  reads `claimed`, and its invocation path is *this* checkout — so the worktree
  lands off the wrong `main`. The prune is **separable** from the materialize loop.
- `claim` is **not** holder-idempotent: re-claiming a task you hold is refused,
  "already claimed by {who}" (`src/change.rs:70`). It stays that way (see Fix 1).
- The canonical, remote-free repo identity is the root commit
  (`git rev-list --max-parents=0 main`): intrinsic to history, identical across
  clones/hosts, distinct per project — balls `91c6469b`, gh-plugin `dd283fc2`,
  adversary `cba55ce3` (three unrelated roots). `Project::at(root)` already shells
  git against the code repo (`src/delivery_repo.rs:23`), so it can read the root.
- The half-close fails in the **right direction by construction.**
  `close.post = ["bl-delivery", "bl-tracker"]` — teardown *then* push
  (`default-config/plugins.toml:25`). The `close.pre` squash to `main` is
  irreversible (rollback declines, `src/delivery.rs`, bl-430e); the archival seal
  is reversible (`Retire` stages the deletion, `src/change.rs:157`; sealed at
  `src/lifecycle.rs:127`; unsealed on a post-abort at `:147`). A rejected push
  (`src/tracker/remote_ops.rs:124`) therefore leaves: squash on `main`, worktree
  torn down, task re-opened → **delivered + open, never done + leftover.** The
  reverse (done + leftover) would need teardown *after* a successful push; the
  hook order forbids it.

## Fix 1 — delete prime's worktree re-materialization (bug #1)

A `work/<id>` worktree is a real filesystem entity; reboots don't wipe it, and if
it is gone the claimant probably isn't working here. The resume-on-prime
justification doesn't hold, so **materialize at claim, nowhere else.** Delete the
`claimed_ids` materialize loop in `bl-delivery`'s `prime.post`
(`src/bin/bl-delivery.rs:120-126`); keep `repo.prune()` (`:128`) as housekeeping.
The `surfaced` path-print for `prime` and the skill/architecture line
"re-materialize the worktrees of any tasks you still hold (prints their paths)"
both go.

- **Re-prime a lost worktree = `unclaim` + `claim`** (committed work on `work/<id>`
  survives the unclaim; a re-claim re-materializes). `claim` stays non-idempotent.

**What doesn't this solve:** a worktree lost *mid-work* drops its uncommitted
changes — exactly as today — and recovery is `unclaim`/`claim`. Marginal and rare
per "worktrees are durable," and the price of the subtraction.

## Fix 2 — claim guards on root-commit repo identity (companion: wrong-repo claim)

`bl create` records the project's root-commit hash on the ball (new metadata; the
`Task::extra` seam, or first-class). `bl claim` computes the current checkout's
root and **rejects on mismatch**, naming the repo the ball belongs to. Delivery
gains the "source repo" it lacks today; the worktree can no longer land off the
wrong `main`, and a remote is never consulted.

- **No override config.** The collision (two repos sharing a root hash — only via
  forking at the root, not a crypto break) *fails open*: a mismatch rejects, a
  match passes, and a colliding match grants an attacker nothing they don't
  already have (they would already be working from that directory). So there is
  no risk worth a knob.

**What doesn't this solve:** a full-history rewrite re-mints the root — as it
re-mints everything and already breaks every clone — so it is handled out-of-band
(balls' own `main` was rewritten 2026-06-27). A ball created before this feature
carries no root and is unconstrained (back-compat). A ball created from a checkout
with no code repo (a pure hub) records nothing → unconstrained.

## Fix 3 — pave the half-close (bug #2), don't prevent it

The direction is already correct (Ground truth). The maintainer's principle:
*a stumbling block walked over often, paved well, beats a rare edge case that
surprises.* So **keep the sequence, keep the frequency**, and don't auto-sync.
Work:

1. **Kill the cosmetic `bl: No such file or directory (os error 2)`** on the retry
   close — delivery touching the already-torn-down worktree. Make
   deliver-from-missing-worktree a clean no-op (the squash already converges,
   bl-430e), so the worn path emits no scary error.
2. **Sharpen the rejection message** so `bl sync` + retry is unmistakable.
3. **Lock the direction with a test** (rejected push ⇒ delivered + open, never
   done + leftover) so a future `close.post` reorder can't silently flip it.

**Rejected alternative (the first pass):** a core-driven pre-reconcile / auto-sync
before the irreversible act. Rejected — it shrinks a common, paved path into a
rare edge case (worse), needlessly demands a remote, and *prevents* a failure mode
that is actually the desirable one.

## Implementation

Three independent balls, filed 2026-06-28 (prime-deletion first — clean, shippable
on its own):

- **bl-c2bf** — delete prime's worktree re-materialization (Fix 1, p2).
- **bl-1ce7** — root-commit claim guard (Fix 2, p3).
- **bl-547f** — pave the half-close (Fix 3, p3).
