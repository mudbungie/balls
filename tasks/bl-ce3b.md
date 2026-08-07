+++
title = "close.post should delete the delivered branch: deferring it to prime leaks one branch per nested ball, forever"
created = 1786071656
updated = 1786071656
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bug"]
+++
`bl close` tears down the work worktree at `close.post` but KEEPS `work/<id>`,
deferring the branch delete to `prime`. For a ball that delivers to the
integration branch that is merely late. For a NESTED ball (a `--subtask-of`
child, which delivers into `work/<parent>` rather than integration) the deferred
cleanup can never fire at all, so the branch leaks permanently.

## Why prime can never collect a nested child

Deletion is gated on `Project::standing()` (`src/delivery_standing.rs:45`):

    if Self::ok(&self.root, &["merge-base", "--is-ancestor", branch, integration])? {
        return Ok(Standing::Settled);
    }
    let Some(delivery) = self.delivered_since_fork(branch, integration, marker)? else {
        return Ok(Standing::Undelivered);
    };
    if self.contained(branch, &delivery)? { Settled } else { Diverged }

- Exit 1 fails for anything squash-delivered: a squash is a re-minted commit,
  never an ancestor.
- Exit 2 scans the INTEGRATION branch for a `[bl-<id>]`-tagged commit. A nested
  child never delivers to integration — that is the whole point of nesting — so
  there is no such commit and never will be. Returns `Undelivered`.
- Exit 3, the content check that would get this right, is only reached when
  exit 2 found a marker. For a nested child it is unreachable.

`prune` then behaves exactly as specified (`src/delivery_prune.rs:57`):
"Committed-but-undelivered work SURVIVES". It is not failing — it was handed
"undelivered" and correctly refused. The classifier conflates DELIVERED
SOMEWHERE ELSE with NOT DELIVERED.

Observed 2026-08-05: five orphan `work/*` branches, ALL five `bl-chore`
"Update the docs" children, ALL five with their parent's id in the delivered
commit (`work/bl-a04c` -> `[bl-a1a4]`, `bl-f389` -> `[bl-dede]`, `bl-5808` ->
`[bl-6b46]`, `bl-a69c` -> `[bl-1ec6]`, `bl-d36c` -> `[bl-739b]`). Zero commits
on main tagged with any child id; one for every parent. All five verified
content-incorporated and deleted by hand. That was three days of accumulation.
This repo wires `bl-chore` at `claim.post`, so the rate is one permanent orphan
PER CLAIMED BALL. bl-292d was filed when 52 branches had accumulated and prune
fixed it — for balls that deliver to integration. Nesting reopened the same
monotonic growth for balls that do not.

The leak fails safe (the wrong answer is "keep the branch", never "delete
unlanded work"), but the noise is not harmless: each leaked branch then hits the
bl-c117 debris report, whose `contained()` check decays as main moves on, so a
7-line stale-docs branch presents as "content is NOT contained in main" over a
three-dot diff showing 526 lines. Two of the five looked like lost work and were
not.

## The fix: act on the fact where it is KNOWN

Move the branch delete into the close itself, at `close.post`:

    ("close" | "unclaim", "post", false) => repo.release(spec.worktree),

`release` keeps the branch. `Repo::discard` (`src/delivery.rs:54`) — "remove the
worktree AND delete `branch`" — already exists and is already wired for
`rollback claim.post`. The close arm should use it. No new primitive, no new
config, no new verb.

This DISSOLVES the nested case rather than patching it: the closing op knows it
just delivered, so nothing has to reconstruct that fact later from a marker that
structurally does not exist. The root cause is that `prune` is doing archaeology
to recover something the op had in hand.

## NOT at merge time — the ordering is the whole safety argument

    close.pre   -> deliver: gate, then squash work/<id> -> target   <- NOT HERE
                  seal:    store commit deletes tasks/<id>.md
    close.post  -> release: remove the worktree directory           <- HERE

`src/delivery_prune.rs:6` states the deferral's reason: "UNTIL THE SQUASH LANDS
— a close can abort before it (gate failure, stale-source refusal) — the
`work/<id>` branch is the ONLY copy of the diff, and the retry's deliver
recomputes from it." That justification is real and expires at the squash. The
bug is that the deferral was written broader than its own argument and ran all
the way out to `prime`. By `close.post` both the squash and the seal have
landed.

## The test that must exist before this ships

`close.post` is `bl-delivery, bl-tracker` in that order. If `bl-tracker` fails
after the seal and the op unseals, the retry must survive an ABSENT branch.
Reasoning says it does — `deliver` is documented as "a no-op when the
worktree/branch is absent" and the squash is already on the target, so the retry
no-ops the delivery and re-seals. UNTESTED. Write it: kill the tracker
post-seal, prove the retried close converges and the code is on the target
exactly once.

Take this seriously rather than as a formality. "Absent branch => empty
deliverable => proceed" is correct ONLY because the squash already landed.
Reached one step earlier the identical code path silently delivers NOTHING —
quiet data loss, no abort, no diagnostic. That silent failure is precisely what
the blanket deferral was buying insurance against, and this ball is cancelling
that insurance. The test is what replaces it.

## Scope note

This SUPERSEDES rather than complements bl-58b8 ("Design: let prime prune
closed+contained work/ branches (widens prime's deletion license)"). That ball
proposes teaching prune better forensics; this one removes the need for the
forensics. Deliberately NO edge filed against bl-58b8 — it is claimed and in
flight, and gating another agent's ball from outside is not this ball's call.
Whoever takes either should read the other first.

prime's prune STAYS, as the backstop for a crash between the seal and
`close.post`. It becomes the rare path instead of the routine one.