# bl-7b71 — Nested delivery: the target ref

**Status: CONVERGED by dialogue 2026-07-21. The MECHANISM is IMPLEMENTED (bl-ad5d):
the target derivation lives in `src/target.rs`, rides the §7 `Command.target`
(`src/wire.rs`), and the delivery plugin consumes it as
`target.map(work_branch).unwrap_or(integration()?)`
(`delivery::target_branch`). The `--subtask-of` claim-gate → close-gate flip is
IMPLEMENTED too (bl-e844): the sugar mints `--parent E --blocks close`, so the
everyday subtask spelling IS the nesting declaration. The rendered target column
(bl-6915) is IMPLEMENTED. Nothing here is open.**

**AMENDED 2026-08-02 (bl-a1a4).** Where this doc says a close *folds the target
in*, it no longer does. Delivery is a validation and atomic-advance boundary,
never a merge queue: for every edge `S -> T` it pins `P = tip(T)` once and
REQUIRES `P` to be an ancestor of `tip(S)` already, refusing a stale source
before any merge, gate, squash or ref move. Reconciling is the source owner's
job, in the source worktree, tested there. This makes the design's own claim
sharper rather than weaker — the rule was always meant to be one operation at
every depth, and it now is one operation with no hidden merge inside it. The
recursion carries the refusal too: a sibling closing into an epic makes the next
sibling's source stale exactly as a landing on main does.

## What is actually hardcoded today

Not the name `main`. Delivery resolves its destination per op with
`git symbolic-ref --short HEAD` on the project repo
(`delivery_repo_acts.rs::integration`) — whatever branch that repo's HEAD
points at. In the common bare deployment HEAD is a settable symref, so
`git symbolic-ref HEAD refs/heads/dev` already retargets every delivery
repo-wide, with no code change.

What is hardcoded is that there is exactly **one** destination, repo-global,
for every task. This design does not make the target dynamic — it already is.
It makes the target **per-task**. That is the whole delta.

## The rule

Every task delivers to a **target ref**, derived at op time, never stored:

> If the task close-gates its live parent — it has `--parent X` AND X carries
> the blocker `{this, on: close}` — the target is `work/<X>`.
> Otherwise the target is the integration branch (main).

`claim` forks the target ref. `close` requires the target to be incorporated
already (bl-a1a4), runs the repo's pre-commit hook on that exact tree, and
plumbing-advances the target to the tagged squash by CAS.
Today's flat behavior is the degenerate case: no nesting parent → main. Depth
recurses naturally (a gate under a subtask under an epic targets the subtask's
ref, and so on up to main at the root).

The full delivery target is a tuple **(root, ref)**. Federation pins the root
(distinct root_commits ARE the fleet; root-SET admit keeps an agent out of the
wrong repo). This design pins the ref. Both coordinates derive from existing
facts — the store never grows a field.

## Why: composition (the general case)

`bl` today can express *containment* — `--parent` says a ball belongs to an
epic — but not *composition*: the children still deliver to main one at a
time, so an epic is a label on a report, never a thing that exists in git.
Under a per-task target it becomes a ref. Concretely, that buys:

- **An epic merges as a whole.** Five children accumulate on `work/<epic>`;
  the epic delivers once. One hook run on exactly what lands, one commit on
  main, one reviewable unit — today a PR can only ever be a single ball,
  because each ball squashes to main independently.
- **A release is an epic.** There is no release feature to build and none
  should be: composing a release *is* contributing to a branch, which is what
  this already does. A release ball with children is a release branch, and it
  lands whole. `bl` stops paraphrasing git and starts using it.
- **Decomposition gets cheaper.** Splitting a task into subtasks today means
  choosing between parallelism and a coherent landing; with a target ref you
  get both, so the pressure to file one fat ball goes away.

The unifying statement: **"done" stops meaning "on main."** It means
*delivered to my target*, and main is simply the target of a ball that has no
parent. Everything below is the consequence of that one sentence.

## Why: the verification gate (the case that surfaced it — bl-54fe)

This is the narrow case. It is worth keeping because it is the bug that
started the ball and because it falls out for free, but it is not the
justification: composition is.

A verification gate wants to be *claimable once the parent's work exists, but
before the parent delivers*. When close IS delivery-to-main, that state is
unexpressible, so agents approximate and both approximations are wrong:
`--parent X --blocks close` alone is vacuously satisfiable (the gate forks
clean main, verifies nothing, closes), and adding `--needs X` is a
claim/close cycle that `bl list` renders as a healthy ready/blocked pair —
the deadlock springs at `bl close`, after the work. Observed recurring
(lernie, 2026-07-11: 15 mis-wired tasks).

Under nesting, the documented spelling **becomes correct as written**: the
gate's claim forks X's *live branch*, verifies against the work it gates,
merges its findings back into X's branch, and X closes gated on it. No repair
edge, no cycle. The footgun was never the graph — it was that claim only knew
how to fork main.

## What core already provides

- **Delivery is ref-addressed plumbing.** `delivery_repo.rs` recomputes every
  act from `(path, branch)`; the squash is `commit-tree` + `update-ref` and
  never disturbs a checked-out integration tree (§11/§14, bl-ee85). Main is
  just the constant currently passed as `branch`.
- **The worktree is a cache, the ref is the fact.** The worktree path is
  computed, never stored (§7). No filesystem↔branch 1:1 is needed — one
  worktree per *claim*, zero worktrees required for a ref to be forked from
  or merged into.
- **Close delivers committed WIP regardless of claim state** (bl-65e0). An
  epic's accumulated branch delivers with one `bl close`, no standing
  claimant session.

## Settled by dialogue (2026-07-11)

### Lazy ref mint — not worktree materialization, not refusal
A child claim needs the parent's *ref*, not its worktree. If `work/<epic>`
does not exist, the first child claim mints it as a bare ref at main's head:
nothing to orphan, no filesystem debris, nothing a missed op can lose (the
objections to auto-materialization were worktree objections). Children merge
in by plumbing; the epic never needs a worktree, so **the branch is the
holder** — no agent carries the epic's lifecycle in context. The epic is
claimed only for actual integration edits, and closed by whoever closes last
(or a chore).

Note this is a *naming*, not a mechanism: forking main and merging back into
a freshly-minted `work/<epic>` is bit-identical to minting at main's head and
forking that. There is no new mint code path — only the point at which the
name starts existing.

### The offramp is the existing explicit signal — no knob, no flag
The close-gate edge IS the nesting declaration. Bare `--parent` (containment
only, gates nothing) stays flat-to-main — a per-child offramp that deletes no
config when unused. Nesting requires BOTH coordinates: parent pointer + a
close-gate on that same parent. An explicit `--blocks ID:close` on a
non-parent remains pure enforcement and never redirects delivery.

The parent pointer earns its keep here, and the cheaper rule ("close-gates
exactly one task ⇒ deliver into it") is wrong: two sibling features where one
must land before the other are spelled with a bare close-gate and must keep
delivering independently to main. Containment is what licenses redirection.

### `--subtask-of` flips from claim-gate to close-gate
The one true behavior change: the everyday sugar must produce the nesting
edge, because "epic claimed first, children deliver into it, epic's close
gated on children" inverts today's "epic claim-blocked until children close."
The flip is self-migrating: existing claim-gated children carry no
close-gate, so pre-existing epics keep flat delivery untouched.

### Distribution is a plugin; core stays upstream-agnostic
Core touches only local refs. A bl-remote-style plugin — `claim.pre` fetches
`work/<parent>` from the configured upstream, `close.post` pushes the updated
target ref (`-u` at first mint) — makes a distributed epic work natively. A
push race between clones fails at `close.post`, the *intended* half-close
failure direction (local seal is the binding commit point; retried close
converges, bl-c3c0). Federated wrong-repo pickup composes as the same
mechanism: an upstream on the target ref of a root the fleet resolves.

### bl-chore's endpoint
Impl becomes a sibling like test/doc. The epic is a pure integration
container; every child delivers into the epic branch; one main-delivery, one
hook run on exactly what lands.

## The core↔plugin seam: one optional wire field

The derivation is a **graph** fact and delivery is a **plugin**. The plugin is
"kind-blind & stateless across ops — it NEVER branches on task kind" (§11);
its branch and worktree path are pure functions of `(binding, id)`. It has no
business reading the blocker graph, and it *could not* read it correctly
anyway: the close-gate edge lives on the PARENT's task file, not on the ball
riding the wire (`before: Option<Task>` carries the ball's own state only). A
plugin that opened `binding.store` and re-derived nesting would fork core's
graph semantics into a second home.

So: **core derives the target and puts it on the wire; the plugin consumes
it.** Concretely, one optional field on the §7 `Command` (the op plus its
intended diff — op-scoped, present at both `pre` and `post`, absent on the
diffless checkout-lifecycle ops):

> `target: Option<String>` — the **id** of the ball whose branch this op's
> delivery forks from and folds back into. Absent = the integration branch.

Three properties earn it:

- **Absent is today's payload, byte for byte.** `skip_serializing_if` on the
  `None` leaves every existing wire shape unchanged — the same discipline
  `stealth: bool` already follows (bl-9df0).
- **It carries an id, not a branch name.** `work/<id>` is the plugin's formula
  (`delivery_path`); core spelling it out would be a second home for the
  naming. Core says *which ball*, the plugin says *which ref*.
- **`Repo::integration()` survives as the default**, not as a rival: the
  plugin's whole rule becomes `target.map(work_branch).unwrap_or(integration()?)`.
  No new env var, no return channel, no plugin-side graph read. Net mechanism
  is flat — one wire field in, one hardcoded assumption out.

**`prime` pruning needs no target awareness.** Prune keeps a `work/<id>`
branch whose delivery is not yet contained in the integration branch — so a
child closed into an epic is simply *unsettled* until the epic lands on main,
whereupon it settles and prunes on the next prime with zero new logic. The
conservative existing test is already the correct nested test.

> **FALSIFIED by bl-ce3b (2026-08-07).** The conclusion held; the reasoning did
> not. A nested child never *becomes* settled: `Standing` proves delivery by
> finding a `[bl-<id>]` commit on the INTEGRATION branch, and a child that
> delivers into `work/<parent>` never writes one there — so its branch read
> `Undelivered` forever and leaked, one per closed child. The fix was not
> target-aware forensics in prune but `close.post` deleting the branch itself,
> where delivery is a known fact rather than a reconstructed one. Prune is still
> target-blind, now because nested children are gone before it enumerates.
> See `docs/architecture.md` §11 (nested delivery) and §14.

## Consequences the doc must own

### The hook runs at every close, children included (was open Q1 — resolved: uniform)
One rule — require the target incorporated, run the repo's pre-commit hook on
that tree, advance the target — at every depth, root included. The argument is **attribution**,
not symmetry: a child that breaks clippy or coverage fails in its own
worktree, at its own close, in front of the agent that caused it. Root-only
defers every breakage to a tree assembled from many agents' work, surfacing at
the close of whoever happens to go last — the worst place to debug it and the
wrong person to hand it to.

The root hook is **not** thereby redundant. Children A and B can each pass
alone and fail merged (semantic conflict; coverage of a path only B deletes);
the root run is the only one that sees the union. Uniform means every close
gates its own target, and the root's target is main.

The cost is honest: N children × the repo's full hook (~1 min here) plus the
root run. That is the price of attribution, and it is paid in parallel by
whoever incurs it.

### A closed child is delivered, not landed (was open Q2 — resolved: no new mechanism)
Under nesting, `bl close` means what it always meant — *delivered to its
target* — and for a child the target is not main. The temptation is a notice,
a "provisional" state, or a refusal. All three store what is computable:
whether a ball's work is on main is `git merge-base --is-ancestor` against its
delivery tag, exactly as it is today (close has never meant "on your origin").
Nesting makes an existing gap structural rather than occasional; it does not
create a new kind of fact.

What *is* new is a destructive act with a wider blast radius: `git branch -D`
on a live epic ref discards the delivered work of every closed child, not just
one agent's WIP. That act already sits outside `bl` (unclaim keeps the branch;
prune deletes only settled ones), so it stays a documented hazard, not a new
guard rail. If it bites in practice the fix belongs in prune's settledness
test, which already knows the difference.

### "Any checkout of a moved ref is stale" — the general form
SKILL.md today teaches a special case: delivery moves *main* by plumbing and
never touches the root checkout, so a non-bare root is stale after a close.
Under nesting the same sentence covers the epic — if an epic is claimed and
has a worktree, a child's close advances `work/<epic>` underneath it and that
worktree is stale until refreshed. This is a **reframe, not a new caveat**:
the doc sweep should raise the existing rule to its general altitude ("a
delivery advances a ref by plumbing and never touches a checkout of it") and
let main be the instance.

## What this does not solve

**The timing half of bl-54fe.** A gate child is `ready` the moment it exists;
its claim forks whatever `work/<parent>` holds, which may be an empty ref at
main's head. Nesting makes verification *meaningful when performed* — the gate
sees the parent's real work and merges findings back — but it does not make it
*correctly timed*. "Claimable once the parent's work exists" stays
unexpressible, and a stranger can still pick up a gate mid-flight and verify a
half-done tree. That is dispatch signaling (bl-6c84), and it is a strictly
softer failure than today's fork-clean-main vacuity.

## Accepted costs (raised 2026-07-21, judged worth it)

### Siblings reconcile against each other, one at a time (bl-a1a4)
Under the ancestry precondition, N children of one epic closing concurrently
serialize: whoever lands first makes every other source stale, and each of the
rest merges the epic's new tip in and re-tests before closing. That cost is
real and it is the point. The alternative is delivery merging on their behalf
and gating a tree none of them built — which is how a clean automatic fold ships
a semantic conflict, and how a resolvable one silently picks a side. The work
does not disappear either way; the precondition puts it in front of the agent
who owns the branch, in the worktree where it can be run, instead of inside a
close where it cannot. No lease, no merge queue, no refold loop, no new verb —
one `merge-base --is-ancestor` and a refusal that says what to do.

### A long-lived target stretches "closed" past comfort — render it, don't store it
The "delivered, not landed" gap above is benign at epic scale (hours) and
strained at release scale (weeks): a ball closed into a release that ships
next month is, for that month, invisible — absence is its whole record. The
answer is still not a stored field. It is a **rendered column**: `bl list`
shows a ball's target when that target is not HEAD, exactly as the root-aware
`--everywhere` labels are render-only decoration over a derived fact
(bl-0161). The query surface stays schema-complete; only the projection grows.

**IMPLEMENTED (bl-6915), and both open questions resolved by subtraction.**
`src/reads/target.rs` derives the target from the already-loaded catalog (the
decision itself stays in `crate::target::close_gated` — one home), so a listing
pays no IO and no git per row. `bl list` renders it as a trailing `  ->bl-xxxx`
on live and dead rows alike; `bl show` renders it as a `delivers` field under
`parent`. `--json` is untouched on both.

- *Does the decoration belong on `bl show`?* **Yes** — it is the same derived
  fact at the same cost (nothing), and `show` is where the coordinate that turns
  bare containment into nesting is read, right under `parent`.
- *Is a landed-vs-delivered marker on the CLOSED side worth a git query per
  row?* **There is no git query, at any depth — the column already IS that
  marker.** A target derives only against a LIVE parent, so on a closed ball a
  rendered target means "delivered, not landed" and its absence means "landed".
  Where the work actually is, is then an ordinary graph read: follow the target,
  which renders its own target, up to the parentless ball whose target is the
  integration branch. A `merge-base --is-ancestor` against a delivery tag would
  re-derive the delivery PLUGIN's tag naming inside core to re-answer what the
  ball graph answers already.

### Depth costs hook runs — two levels is the shape, deeper is a smell
The uniform hook means the repo's pre-commit gate (clippy + line cap +
tarpaulin, ~1 min here) runs at every level as work propagates up:
ball → epic → release → main pays it three times. That is the price of
attribution and it is correct, but a mid-level gate run is progressively less
meaningful the further it is from main. **No knob** — the honest statement is
that ball-under-epic-under-main is the intended shape, arbitrary depth is
supported because the recursion is free, and nesting three deep should read
as a decomposition smell rather than a feature to tune.

## Still open

1. ~~**`--subtask-of` doc/migration sweep.**~~ DONE (bl-e844): SKILL.md,
   `skill/create.md`, `skill/close.md`, `skill/import.md`, §9/§10/§16 of
   `docs/architecture.md`. The one place claim-gate semantics deliberately
   SURVIVE is §16's legacy epic reciprocal edge — reproducing what the old store
   meant, so a migrated epic keeps flat delivery.
2. **`--subtask-of`'s age split.** Old subtasks carry no close-gate, so they
   keep flat delivery and two identically-spelled epics behave differently by
   creation date. Self-migrating and harmless, but `prime` is the established
   home for version-skew convergence (bl-18bf) if it ever needs converting.
   Deliberately not decided here — decide it when an old epic actually bites.
3. **Dispatch of mid-flight gates** — bl-6c84, cross-referenced above.

## Relation to bl-54fe

bl-54fe keeps its independently-valuable fixes regardless of this design:
write-time cycle refusal at the §10 front door (bl-6b8c's `require_live` is
the slot), and a create-skill note on gate topology. Cross-referenced, never
gated on this ball.
