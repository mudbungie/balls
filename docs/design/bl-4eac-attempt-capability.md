# bl-4eac — the attempt: a delivery source that is not a ball

Status: **converged, implemented** (2026-08-06). Amends architecture §11 (§11.1).
Upstream ruling: yog `docs/VISION.md` §4.10, "The project-delivery contract — one
recursive graph" (yog bl-2b8c, 2026-08-03). Local prerequisite: bl-a1a4 (the
source owner incorporates the target; delivery validates and CAS-advances,
never reconciles).

## The maximal-removed version, stated first

**Nothing new is needed. balls already owns the whole mechanism.**

Read the delivery acts and the claim is nearly true. `Repo::mint` makes a source
ref at an exact base. `Repo::materialize` makes a private worktree on it.
`Repo::deliver` pins the target, requires it incorporated (bl-a1a4), gates the
exact tree with the repo's own `pre-commit`, mints a tagged squash and CAS-moves
the target. `Repo::release` drops the worktree and keeps the ref;
`Repo::discard` drops both. `Standing` converges a retried delivery onto a
squash that already landed. `worktree prune` heals a worktree directory a crash
took. Every one of those is policy-blind already — the plugin "NEVER branches on
task kind" (§11).

So the honest deliverable is not a subsystem. It is **two absences**:

1. every one of those acts is reached only through a *ball* — `claim`/`close`
   derive `(worktree, branch, target, marker)` from `tasks/<id>.md` and the §7
   wire, and the acts themselves are `pub(crate)` behind a plugin edge;
2. `deliver` returns `()`. It computes the base, the source tip and the delivery
   commit and throws all three away.

Fill exactly those two and the capability exists. Everything below is the
smallest shape that fills them without inventing a second delivery path.

## What is *not* built, and why

Named so a later reader does not re-propose them:

- **No `bl attempt` verb, no task status, no blocker kind.** An attempt is not a
  unit of work balls tracks; it is a unit of *delivery* balls performs. Giving
  it a verb would make it a ball with extra steps, which is precisely the
  manufacture bl-4eac exists to avoid.
- **No candidate / winner / cohort / outcome field.** Acceptance is the target's
  own history: the accepted attempt is the one whose `[handle]` squash the
  target carries. Cohort is `(target, base)`. Rejection is the *absence* of a
  delivery. A stored winner would be a second home for a fact git already keeps
  (§0 derive-don't-store).
- **No fan/judge policy, no merge queue, no retention timer.** N, comparison,
  accept/reject/rework and *when a loser expires* are yog's. balls exposes
  `release` (worktree goes, source stays addressable) and `discard` (both go)
  and holds no opinion about which to call.
- **No liveness probe on the lease.** bl-1e98 refused claim liveness and that
  refusal stands here (see "The lease is git's own" below).
- **No duplicate project bytes.** An attempt is a ref and a worktree in the
  project repo. Nothing is copied anywhere.

## The shape

Three types, in `src/attempt.rs`.

### `Target` — opaque, balls-resolved, pinned on use

```rust
pub struct Target(String);   // the branch name; the field is private
```

A caller cannot *construct* one — it can only *ask balls for* one, three ways,
which are exactly §4.10 item 1's three cases:

| ask | authority | what it is |
| --- | --- | --- |
| `Project::target(None)` | the project repo's own `HEAD` branch | the integration branch — never hardcoded to `main` |
| `Project::target(Some(id))` | the ball graph | `work/<id>`, lazily minted at the integration head — the same ref `bl close` derives for a close-gating child (bl-7b71) |
| `Project::target_ref(branch)` | an explicit, *validated* branch | the "explicit repo + ref" start; a missing branch is refused by name |
| `Attempt::target()` | a parent attempt | its own source ref — a write-capable child targets its parent's source, the fractal law at one more depth |

The private field is what keeps item 1's promise ("callers never construct
worktree paths or ref names") *mechanical* rather than a documented request.
`Attempt::target()` is the reason the source ref never has to be exposed as a
string: the one legitimate use of an attempt's ref name is as another attempt's
target, and that use is expressible without ever spelling it.

`target_ref` validating existence is also item 6's **deletion/move** coverage on
the entry side; a target that moves or vanishes *after* resolution is caught on
the exit side by the ancestry precondition and the CAS.

### `Attempt` — a private, write-capable source

```rust
Attempt::open(root, xdg, &target)            -> Attempt   // fresh handle
Attempt::resume(root, xdg, &target, handle)  -> Attempt   // after a crash
attempt.handle() / .worktree() / .base() / .target()
attempt.deliver(summary, note)               -> Delivered
attempt.release()                            // worktree goes, source ref stays
attempt.discard()                            // both go
```

- **Handle.** `at-` + 8 hex, minted by the ordinary `IdScheme` re-rolled off the
  live `attempt/*` refs. Opaque to the caller; the only name it holds. lernie
  binds an agent to it, yog stores it in its own history — neither ever learns a
  path or a ref.
- **Namespace.** The source ref is `attempt/<handle>`. `work/*` remains ball
  identity, per the ruling. A useful consequence falls out for free: `prime`'s
  settled-branch prune globs `work/`, so **attempts are exempt from balls'
  automatic cleanup by construction** — retention is yog's, and balls needed no
  flag to stay out of it.
- **Worktree.** `$XDG_STATE_HOME/balls/attempts/<invocation_path>/<handle>/` —
  the invocation path MIRRORED verbatim, not percent-encoded, for the same
  bl-f3e4 reason the delivery worktree mirrors it: this is a cargo build dir and
  `rust-lld` cannot open an output file under a `%` ancestor.
- **Base.** `merge-base(target, source)`, never a stored field. For a fresh
  attempt that *is* the tip the ref was minted at; for a resumed one it recovers
  the true fork point rather than re-pinning to a target that has since moved.
  One formula, both cases — the special case dissolves.

`open` and `resume` share one body: ensure the ref (mint at the target tip if
absent), materialize the worktree (create-if-absent), derive the base. `resume`
adds one guard — an unknown handle is refused rather than quietly minted, so a
typo cannot become a new attempt.

### `Delivered` — the provenance return

```rust
pub struct Delivered {
    pub target: String,          // the ref advanced
    pub base:   String,          // the PINNED target tip: squash parent, CAS old-value
    pub source: Option<String>,  // source tip at delivery; None ⇒ nothing was ever authored
    pub commit: Option<String>,  // the delivery commit; None ⇒ nothing landed
}
```

Both `None`s mean one thing between them: *the target already contained
everything the source had*. `source: None` is the never-authored ball (a claimed
non-deliverable); `commit: None` is the empty deliverable or the fully-merged
source. A converged retry returns the **standing** delivery commit — the one an
earlier aborted close already landed — not `None`, because provenance wants the
commit that exists, not the fact that this call did not mint it.

This is the whole of item 5. Nothing is stored to produce it; every field is a
value the delivery already computed and used.

## The delivery law is shared verbatim

`deliver_close` (the ball path) and `Attempt::deliver` (the non-task path) both
funnel into one function, `delivery_message::deliver_to`. They differ in exactly
what they know and nothing else:

| | ball path | attempt path |
| --- | --- | --- |
| source ref | `work/<id>` | `attempt/<handle>` |
| target | derived from the ball graph (§7 wire) | the `Target` the caller opened against |
| subject | the ball title, tagged `[<id>]` | the caller's summary, tagged `[<handle>]` |
| marker | `[<id>]` | `[<handle>]` |
| body | `-m` note + the source's own commit messages | the same |

Everything downstream — half-merge guard, capture, retry standing, the bl-a1a4
ancestry precondition, the gate, the no-resurrection invariant, the tagged
squash, the CAS, the reconcile — is one code path with no attempt/ball branch in
it. That identity is the deliverable, not a nice property of it: an attempt that
delivered by any other route would be a second delivery law, and two
representations of one law drift.

The recursion is likewise free. A parent attempt's source ref is a child
attempt's target; a sibling that lands first makes every other sibling *stale by
construction*, and bl-a1a4 refuses it until its owner incorporates the new
target in its own worktree and tests there. Sequential synthesis is what the law
already does — it needed no primitive.

## The lease is git's own

Item 2 asks for a single-writer lease. balls adds no lockfile and no liveness
probe, because git already enforces the only thing a lease can honestly
enforce:

- a handle is minted fresh and re-rolled off the live `attempt/*` set, so two
  attempts never name one ref;
- `git worktree add` refuses a ref already checked out in another worktree, so
  two worktrees never share one attempt's index;
- the worktree path is a pure function of the handle, so an attempt has exactly
  one place to be.

What remains — one *caller* handing one handle to two agents — is not detectable
without a liveness probe, and bl-1e98 already ruled against inventing one
(a probe that can be wrong is worse than an invariant the caller owns). balls
never returns a handle twice; who holds a returned handle is the caller's fact.
This is the same answer the claim lease gives, at the same altitude.

## Crash convergence, without a reaper

Three shapes, all already-existing behaviour:

- **worktree directory gone, ref alive** — `resume` re-materializes; the stale
  registration a bare `worktree add` would trip over is cleared by the
  `worktree prune` `materialize` already runs (bl-b404).
- **squash landed, process died before the caller recorded it** — the retry
  standing detects the `[handle]` commit fork-scoped and converges, returning
  it. Content-containment, not commit-presence, is the predicate (bl-c231), so a
  source carrying work *beyond* its delivery aborts loudly instead of being
  silently stranded.
- **attempt abandoned entirely** — nothing to converge. The ref is inert
  history; the worktree is a directory. `discard` removes both when yog's
  retention says so. balls does not sweep them, which is the same statement as
  "yog owns retention" made mechanically.

## Library/binary parity

Item 7. The capability is a **library** surface (`balls::attempt`), reachable
from a linking host exactly as `delivery_bin::run` already lets a host *be* the
`bl-delivery` sibling. There is no CLI counterpart and must not be one: a verb
would be a second entry point to a capability whose whole point is that the ball
path and the attempt path are one mechanism. The `bl` binary reaches it through
`bl close` (the N = 1 ball attempt); yog reaches it through the crate. Same code,
two callers — which is the parity the ruling asks for.

## Coverage (item 6)

| shape | where |
| --- | --- |
| target deletion / move | `target_ref` refuses a missing branch; a moved target refuses at the ancestry precondition, and mid-delivery at the CAS |
| bare repositories | open + deliver against a `--bare` project repo |
| concurrent attempts | two attempts on one target: the first lands, the second is refused as stale until it incorporates |
| retry after crash | worktree removed under a live attempt → `resume` → deliver converges on the standing squash |
| stale target | the bl-a1a4 refusal, named S / T / P |
| gate failure | a failing `pre-commit` aborts before the seal; the target has not moved and the source is intact |
| rejected retention | `release` leaves the source ref addressable and the target untouched |
| explicit discard | `discard` removes worktree and ref; a re-`resume` of that handle is refused |
| claim/close parity | the ball path returns the same `Delivered` identities the attempt path does |

## What this does not solve

- **Cross-repo attempts.** An attempt lives in one project repo. A change
  spanning two repos is two attempts and two deliveries; balls offers no
  transaction across them and should not (bl-0161 already rules cross-repo work
  as separate balls).
- **Comparing attempts.** balls returns identities; `target..source` is a plain
  git read anyone can do. Deciding which diff is *better* is yog's, by
  construction of the layer law.
- **Reclaiming a handle's storage automatically.** Deliberate. If a caller loses
  a handle, the refs are still enumerable (`git branch --list 'attempt/*'`) and
  the worktrees still listed (`git worktree list`) — inspectable, never swept.
