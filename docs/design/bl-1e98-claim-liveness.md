# bl-1e98 — claim liveness: staleness is a derived view, not a lease

**CONVERGED by maintainer dialogue, 2026-07-04.** Maintainer's convergence
statement (verbatim): *"Agreed on your reaper/staleness reads; policy not in
core, just the surface to let a policy get executed: allow a query of what is
old, don't automatically do anything about it. I'm okay leaving reaper unbuilt
for now. Tighten the SKILL.md note."* Positions were stated subtraction-first:
the maximal-removed design leads (nothing in core — no new verb, flag, field,
lease, or reaper), and each addition had to argue its way back in.
Settled-vs-open ledger at the bottom.

This ball was `--needs`-gated on the query surface (bl-8ab5), which CONVERGED
today. That is not a coincidence: the maintainer's read (verbatim, 2026-07-01)
was *"this is actually tied pretty deeply to [the query surface]. With a better
query surface, this kind of thing falls out pretty readily."* It does. Read
`docs/design/bl-8ab5-query-surface.md` first — this design consumes its Q5
directly and adds almost nothing to core.

## The problem (as the ball frames it)

A crashed or abandoned agent holds its claim forever. There is no stored claim
timestamp, no lease, no TTL, no derived "stale" state. Recovery today is the
manual orphan-takeover: commit WIP, tag, `bl unclaim`, re-claim, cherry-pick.
For unattended fleets the ball calls this "the #1 operational hole: readiness is
a query, staleness is not."

The subtraction-first reframe: **make staleness a query too.** Everything the
hole needs is already in the store; the fix is a derived render plus a documented
convention, not a mechanism.

## Ground truth — verified against the worktree (main, 2026-07-04)

Every claim in the ball body checks out against the code, with two corrections:

- **`claimant` is the SOLE occupancy field; there is no claim-time field.** The
  §3 schema (`src/task.rs:22-58`) stores `created`, `updated`, `claimant`
  (`Option<String>`, "present ⇒ claimed, absent ⇒ not"), `root_commit`,
  blockers, tags, extras. No `claimed_at`, no `lease`, no `status`.
- **`updated` is restamped by EVERY mutation.** `Occupancy::stage` and
  `Update::stage` both do `task.updated = self.now` unconditionally
  (`src/change.rs:88,143`). So `updated` is last-touch, NOT claim time — the ball
  is right.
- **Claim time already exists, derivable.** It is the timestamp of the newest
  commit touching `tasks/<id>.md` whose §5 trailer is `bl-op: claim` — settled
  and verified in bl-8ab5 Proposal 2. The trailer is emitted by
  `src/message.rs:86` (`bl-op={token}`, normalized to `bl-op: claim` by
  interpret-trailers). Nothing is stored.
- **`unclaim` checks NO identity.** `Occupancy::stage` for the unclaim branch
  runs only the blocker gate (`enforce::gate`, `src/change.rs:85`) then clears
  `claimant` — it never compares the actor to the current claimant. `--as` sets
  the recorded actor (`src/mutate_args.rs:99`) but nothing validates it. Any
  actor can release any ball. **This is what makes takeover possible.**
- **`claim` REFUSES an already-claimed ball** (`src/change.rs:75-81`,
  `AlreadyExists`). This is why takeover needs the `unclaim` step first — you
  cannot reclaim over a live claimant.
- **`close` checks no identity either** — only `closeable` (blockers resolved,
  `src/enforce.rs:41`). A reap COULD close; Q5 argues it must not.
- **No "stale"/"reap"/"lease"/"ttl" token exists anywhere in `src/`** (grep
  clean). This ball introduces the concept; the question is where it may live.

## Q1 — where claim-age derivation lives, and what it costs

**Settled by bl-8ab5; re-priced here against this store.** Age is derived at
render from the claim commit, per-ball:

```
git log -1 --format=%ct --grep='^bl-op: claim$' -- tasks/<id>.md
```

Measured on the largest store on this box (`~/dev/balls`, **676 commits, 208
dead balls**), 2026-07-04:

- One claim-age walk: **~12–29 ms**, whether the claim is recent or the grep
  falls through the whole history (worst case is still ~27 ms — git log is fast
  over 676 commits).
- The walk is per **claimed** ball, and the claimed set is bounded by fleet
  size, not store size — 1–5 live claims per store observed across every clone
  here. A full sweep of a store's live claimed set is **tens of milliseconds**.

The cost is trivial and self-limiting; no cache, no index, no stored field. This
is the §3 derive-don't-store discipline and the bl-8ab5 no-index rule, already
converged. **bl-1e98 must not reopen "derive vs store."**

## Q2 — the surface: it already exists (nothing to add)

**Maximal-removed position: bl-1e98 adds ZERO query surface.** The staleness
dashboard is `bl list -s claimed`, and the age column that makes it one is
already being implemented as **bl-46ef** (bl-8ab5's convergence: age renders
attached to the claimant, `@Name (3h)`). Today the human `list` render prints
the bare `@claimant` as the row's last column (`src/reads/list.rs:107-108`); the
age render decorates exactly there. `show` gains a `claimed <ISO> (<age> ago)`
line the same way (`src/reads/show.rs:112-113`).

The hard layer rule from bl-8ab5 governs everything downstream: **list flags
read STORED frontmatter only**, so a staleness threshold can NEVER be a list
flag — `--stale-over 2h` would smuggle a policy number into core. Age is a
human-render column; `--json` stays bedrock (stored fields only). A machine
consumer (a reaper) derives age with the one-liner above against the store
checkout, exactly as bl-8ab5 sanctions. Nothing here is new.

Under the maintainer's convergence principle (verbatim, bl-8ab5): *"a user can
break into the tasks branch and start poking around, but they shouldn't have to
to do their basic job. A user having to bust open the tasks branch probably
implies a failure of the ergonomic surface."* "Is anything stale?" is a
basic-job fleet question, and `bl list -s claimed` answers it on the surface —
so the surface is complete. A reaper reading the store directly is the sanctioned
machine path, not a break-in.

## Q3 — reaping: nothing in core; a reaper is a plugin, if anyone builds one

**Maximal-removed position: core ships NO reaper, and the paved path is the
documented takeover.** Three levels, cheapest first:

1. **Nothing-in-core + document the takeover (the recommended default).** The
   age column makes staleness visible; recovery is the orphan-takeover already
   in SKILL.md's spirit (`bl unclaim <id>` then re-claim — `SKILL.md:192,330`).
   The fleet ALREADY has a dispatcher: a human/architect session juggling
   agents, who reads `bl list -s claimed`, sees `@dead-agent (6h)`, and runs
   `bl unclaim`. This week that exact path recovered four balls when a whole
   session died. **This is a complete answer.** Ship the age column (bl-46ef),
   document the takeover crisply, and the #1 hole is closed by a query plus a
   convention — no new mechanism.

2. **An OPTIONAL reaper plugin on `prime.post` (only if unattended fleets demand
   it).** If a fleet runs with no human in the loop, automation is policy, and
   policy is a plugin. `prime.post` is the correct and REAL hook: it already
   runs outside any op, after re-materializing the still-claimed set, and is
   where `bl-delivery` prunes settled `work/<id>` branches
   (`src/delivery_prune.rs:1`) and where the tracker settles content
   (`src/tracker.rs:109`). A reaper plugin would, on `prime.post`: read the
   claimed set, derive each claim's age by the Q1 one-liner, and for any age past
   ITS OWN configured threshold, shell `bl unclaim <id> --as reaper`. The
   threshold is the plugin's config (§4 layered) — never core. Core needs no
   change; the capability composes entirely from existing surface (`prime.post`
   wiring + the age one-liner + `bl unclaim`). It is severable to the letter:
   delete the plugin and behavior is bit-identical.

3. **A dispatcher-side loop — REJECTED as a core concern.** The dispatcher can
   run the same `prime.post` logic in its own orchestration script without
   balls knowing. That is fine and needs no design; it is level 1 with the human
   replaced by cron. Core owes it nothing.

**The line:** core ships the age column and the takeover doc (level 1). The
reaper (level 2) is a plugin sketch this doc records but does NOT commit balls to
build — it is the bl-adversary/gh-plugin pattern (capability in a sibling,
policy in its config). New core mechanism for reaping is refused: there is no
consumer core must serve that the surface + `prime.post` + `bl unclaim` don't
already serve.

## Q4 — cooperative occupancy: keep it; no identity check earns its keep

**Maximal-removed position: change NOTHING about `unclaim`. Cooperative
occupancy is a feature, not a bug.** The ball asks whether an identity-checked
`unclaim` is "ever right." Attacked hard, in every variant:

- **Hard identity check (refuse if actor ≠ claimant).** This BREAKS takeover —
  the exact path that recovered four balls this week when their claimant was a
  dead session that will never return to release them. A dead agent cannot pass
  its own identity check; a hard check converts every crash into a permanent
  wedge requiring a `--force` escape hatch, and `--force` is just the
  cooperative unclaim with a speed bump and a smell. The check would defend
  against a case that does not hurt (a live agent's ball being stolen is rare and
  self-correcting — the two agents notice and sort it) while disabling the case
  that does (a dead agent's ball being unrecoverable). Net negative.

- **Soft check (warn-but-allow).** A warning that everyone must click through is
  either ignored (dead weight) or automated-around (the reaper passes `--force`
  or `--yes`, so the warning gates nothing that matters). It adds a branch, a
  config knob for "am I allowed to warn," and a second code path, to protect
  against a keystroke mistake that the audit trail already catches: **the unclaim
  commit records the actor** (`bl-actor`, `src/message.rs:91`), so "who stole my
  ball" is answerable after the fact by `git log` without any pre-flight gate.
  Advisory-only safety that the log already provides is mechanism without a
  consumer.

- **The reframe.** Occupancy is DELIBERATELY advisory (§3: `claimant` is a hint,
  status is derived from it). The `claim` refusal on an already-claimed ball
  (`src/change.rs:75-81`) is the ONE guard, and it is enough: it stops two live
  agents from silently double-claiming (the real race), while `unclaim`'s
  openness stops a dead agent from wedging a ball forever (the real failure).
  These two together are the whole occupancy model, and they are correctly
  asymmetric — claiming is guarded, releasing is not, because the danger is a
  ball with two live workers, never a ball with none. An identity check on
  `unclaim` would make the SAFE direction (release) harder to protect the ball
  from the DANGEROUS direction (double-work) — backwards.

**Conclusion: no identity mechanism, hard or soft, earns its keep.** The audit
trail is the accountability surface; the open `unclaim` is the recovery surface;
together they are why a crash is a `bl unclaim` and not a support ticket.

## Q5 — a reap UNCLAIMS, never closes; and what machine-locality costs

**A reap is `unclaim`, full stop.** Closing a stale claim is wrong two ways:

1. **Close archives work someone may still want.** `close` deletes
   `tasks/<id>.md` (`Retire::stage`, `src/change.rs:191`) — the ball leaves the
   live set. A stale claim usually means "the worker died mid-task," not "the
   task is done"; archiving it loses the task itself, not just the claim. Unclaim
   returns the ball to `ready` for the next agent — the intended recovery.

2. **Close from the WRONG box would deliver, or silently strand, committed
   work.** `close.pre` runs the delivery squash (`src/delivery.rs:112`,
   `deliver_close`). On the dead agent's own box a reap-via-close would squash
   its half-finished `work/<id>` onto `main` — shipping unfinished work. On a
   DIFFERENT box the `work/<id>` branch doesn't exist locally, so `deliver` is an
   empty no-op (`src/delivery.rs:76`) — the ball is archived with the committed
   WIP stranded on the dead box's branch forever, unrecoverable and un-re-claimable.
   Both outcomes are bad; `unclaim` avoids both by keeping the ball live.

**Machine-locality — stated plainly, as the constraint demands.** A store
mutation (clearing `claimant`) is a commit that propagates fleet-wide by the
normal sync. But the worktree teardown does NOT: `unclaim.post` →
`delivery.release(worktree)` removes the LOCAL worktree directory
(`src/delivery.rs:52-55,116`), which is idempotent and filesystem-checked. So a
reap from box B:

- **Frees the ball everywhere** (the store commit propagates) — the ball is
  re-claimable, which is the whole point.
- **Cannot tear down box A's worktree or `work/<id>` branch** — they live under
  box A's `$XDG_STATE_HOME/…/plugins/bl-delivery/<path>/<id>`
  (`src/delivery.rs:161`), invisible to B. They persist on box A until box A's
  next `prime.post` prunes SETTLED branches (`src/delivery_prune.rs`) — and a
  never-delivered branch is NOT settled, so committed WIP survives even that.

**What is lost, and why it is acceptable.** The real recent case: a dead agent
left an UNCOMMITTED spike in its worktree; a reap from another box could not see
or save it. Correct — and acceptable, because **uncommitted work is outside the
store's guarantees by definition.** balls' model is: work in the worktree, commit
early; committed `work/<id>` survives unclaim and a later claim-and-close
delivers it (the bl-65e0 contract, `src/delivery_prune.rs:24`). Uncommitted work
was never in git and no distributed system can reap-and-preserve state that only
one dead machine ever held. The paved mitigation is exactly the takeover: when
box A returns, its stranded branch/worktree is recoverable by re-claim +
cherry-pick; if box A never returns, the committed WIP is on its branch and the
uncommitted spike is gone — the same loss as any uncommitted work on any crashed
machine. The stranded local worktree is *stale-but-harmless like an orphan
worktree — no core rotation, prune is manual* (architecture.md's own words,
§log). The convention that shrinks the loss to near-zero is **commit-early
discipline**, already the house rule — not new mechanism.

## Subtractions — what this design refuses

- **No stored claim-time / lease / TTL field.** Derived from the claim commit
  (Q1); a stored field is a second representation that drifts (§3).
- **No `--stale-over` list flag, no `stale` status rung.** Threshold is policy;
  list flags read stored fields only (bl-8ab5 layer rule).
- **No `bl reap` verb, no reaper in core.** A reaper is an optional `prime.post`
  plugin composed from existing surface (Q3); most fleets need only the age
  column + the takeover.
- **No identity check on `unclaim`, hard or soft.** It breaks recovery to
  duplicate the audit trail (Q4).
- **No close-based reaping.** Unclaim, so nothing is archived or mis-delivered
  (Q5).

## Ledger

**Settled (converged, attack to reopen):**

1. Claim age is derived from the newest `bl-op: claim` commit, per-ball, ~12–29
   ms over a 676-commit store; no stored field, no index (Q1, inherited from
   bl-8ab5 and re-priced).
2. The surface is `bl list -s claimed` with the age column already shipping as
   bl-46ef; bl-1e98 adds no query surface, and a threshold can never be a list
   flag (Q2).
3. Core ships no reaper. The paved path is the documented orphan-takeover
   (`bl unclaim` + re-claim); automation, if wanted, is an optional `prime.post`
   plugin with an owner-configured threshold — the same capability-in-a-sibling
   pattern as bl-adversary (Q3).
4. Cooperative occupancy stands unchanged: `unclaim` keeps NO identity check.
   The `claim`-refusal guards double-work; the open `unclaim` enables recovery;
   the commit's `bl-actor` is the accountability surface. No identity mechanism
   earns its keep (Q4).
5. A reap UNCLAIMS, never closes — close would archive the task or mis-deliver
   its committed WIP (Q5).
6. Machine-locality is accepted: a remote reap frees the ball fleet-wide but
   cannot tear down the dead box's worktree/branch; committed WIP survives on
   the branch, uncommitted work is lost, and commit-early discipline is the
   mitigation (Q5).

**Resolved by convergence:**

1. The reaper plugin (Q3 level 2) stays **UNBUILT**. This doc's `prime.post`
   sketch (Q3, level 2) remains as the record for a future operator who
   actually needs one — sketched, not shipped, and no follow-up implementation
   ball is filed. Building it is deferred until an unattended fleet asks for it
   in earnest; the maintainer: *"I'm okay leaving reaper unbuilt for now."*
2. The takeover recovery is documented as a **tightening of SKILL.md's
   existing abandonment note** (`SKILL.md:328`), not a dedicated `docs/`
   runbook — landed riding this ball's close. It carries: staleness is read
   via `bl list -s claimed`'s claim-age column, never stored, with any
   threshold left to the operator; the same-box takeover (commit WIP, unclaim,
   re-claim, relying on `unclaim`'s deliberate lack of an identity check); the
   different-box case (the ball frees fleet-wide, but the dead box's worktree
   and branch are machine-local and committed WIP is stranded until that box
   prunes); and that a takeover unclaims, never closes.
