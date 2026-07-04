# bl-0161 — cross-repo work: many projects on one center store

**CONVERGED 2026-07-04 — ratified in maintainer dialogue.** The
subtraction-first positions below held under attack. On the core move — no
root→project roster; the fleet is `distinct(root_commit)` over the store's
balls — the maintainer:

> "Config model derived from distinct roots: damn, that's way better. Aligned."

Positions are stated subtraction-first: the maximal-removed design leads, and
each addition argued its way back in. The ledger at the bottom records the
converged resolutions.

The gap: agentic work constantly spans repos (API change + client update;
plugin + host), and balls cannot express it. §12 federation is many clones of
ONE project sharing one store; the working recipe for many PROJECTS on one
store (satellite origin-repoint) is trap-laden and tedious.

## The reframe: cross-repo is already built — two gaps remain

A single store serving N projects needs **no new model**. Every routing
primitive already ships:

- **Balls carry their project.** `create` stamps the checkout's root commit
  (`src/mutate.rs:75` → `Task::root_commit`, bl-1ce7) — remote-free identity,
  intrinsic to history, identical across clones/hosts.
- **Claims already route.** `guard_repo` (`src/change.rs:107`) rejects a claim
  whose recorded root differs from the invoking checkout's, pointing at the
  right project. Fail-open when either side is absent; no override.
- **Cross-project edges come free.** Blockers are ids resolved by existence in
  the store (`Task::ready`, `src/task.rs:186`); in one store, a repo-A ball can
  `--needs` a repo-B ball today. Nothing resolves blockers per-repo.
- **Delivery is keyed by invocation path, not by the store.** `claim`
  materializes the worktree off the repo you invoke from
  (`plugins/bl-delivery/<mirrored-invocation-path>/<id>`); `close` squashes to
  that repo's `main`. The store's location is never consulted (§11/§13).
- **N checkouts already share one branch.** Each repo keeps its own XDG store
  clone tracking the shared branch; consistency is the existing optimistic
  mutate→push with the E5 non-ff reject (§12/§13). The "one project" framing
  in §12 is in the prose, not the mechanism.

The remaining gaps are exactly the two the ball names, and both are
corrections/ergonomics, not model changes:

1. **`bl list` lies.** In repo A it shows repo B's balls as `ready`, but
   `claim` refuses them. Readiness became checkout-relative the day the guard
   shipped; `list` still computes a different predicate than `claim` enforces.
2. **Enrollment is multi-step and trap-laden.** The recipe (origin-repoint, or
   post-bl-d081 `bl conf set task-remote` + `bl prime --install`) is two
   commands with a half-enrolled failure state between them, and the legacy
   global XDG remote shadows every other repo's store (the ~/dev/brazen
   breakage, 2026-06-27).

## Q1 — root→project mapping in config: NO (subtracted)

The maintainer's sketch: the config branch declares which root commits it
operates against; local installs trust that; big-bang enrollment. Attacked:

- **Second representation.** The fleet is already derivable:
  `distinct(root_commit)` over the store's balls IS the set of projects the
  store serves. A declared roster drifts the moment a repo's first ball is
  created without a config edit. The stamps on the balls ARE the fleet.
- **No consumer needs it.** Enrollment doesn't: a satellite knows its own root
  and needs the center's URL, not its permission — the guard is per-ball, and
  a center holding no balls for your root gives you an empty list, not an
  error. Listing doesn't: the scope input is THIS checkout's root, computed
  locally. Big-bang doesn't: adopting config is potential RCE (§0), so consent
  is inherently per-checkout — a roster cannot consent FOR a satellite without
  becoming remote authority, which §0 flatly forbids ("no remote is ever
  authoritative").
- **Both maintainer doubts dissolve with it.** No roster → nothing declarative
  to maliciously declare (enrollment stays pull-only, install-gated), and no
  new config schema to fork the non-git local implementation off of —
  `root_commit` is already `Option`, already fail-open off non-git.
- **Wrong plane.** Config names the store; the store holds only data (§0).
  "Which project is this ball's" is a fact about the ball → it lives on the
  ball. It already does.

**Residue, resolved (see Q2):** the fleet view renders foreign balls by root
hash, which humans can't read. Convergence settled this render-time and
config-free — shadow the hash with the enrolled checkout's directory basename,
short-hash fallback — so no roster is resurrected: the names are derived
locally, never declared. Full resolution under Q2.

### Escape hatch: same repo, two checkouts, operating differently

Raised in dialogue:

> "a user might want two checkouts of the same package on their machine to
> operate differently, so there needs to be an escape hatch."

It already exists, and it is the primitive rather than an override. Every store
binding is **per-clone, keyed by invocation path** (bl-d081 `task-remote`): two
checkouts of one repo can bind different centers, or one can go
declared-stealth. The two axes are orthogonal — the **identity axis**
(`root_commit`) answers "which project is this ball's"; the **binding axis**
answers "which store does this checkout talk to." Different questions, so
separating the checkouts needs no new override; the binding already differs.

One implication, stated honestly: two same-repo checkouts bound to the SAME
store are interchangeable claim targets — same root, same admit set — so
differentiation there is by binding only. That is correct, not a hole: identical
identity plus identical store is one work pool by definition.

## Q2 — root-aware listing: list shows what claim admits

Status is DERIVED (§3), and `claim`'s derivation now includes the root guard.
The fix is not a new filter; it is making the derived view honest — **one
shared predicate**: expose the guard's admit test (ball root absent, checkout
root absent, or equal) and have `list`'s default set be the admitted set.

- **Default scope:** balls whose `root_commit` matches this checkout's root,
  PLUS rootless balls (claimable anywhere by the fail-open guard — the
  store-wide set: chores, meta, pre-guard legacy). A rootless CHECKOUT (non-git
  dir, pure task-list use) admits everything, so it sees everything —
  single-store usage is byte-identical to today.
- **Fleet view:** one flag lifts the root scope (`bl list --everywhere`, name
  negotiable), rendering foreign rows with a short root marker. Alternatives
  attacked: overloading `--all` conflates the dead-reach axis with the root
  axis ("ready across the fleet" would drag dead balls in); a derived
  `elsewhere` status rung collapses a foreign ball's real rung
  (ready/claimed/blocked at its home), which is exactly what a fleet
  dispatcher wants to read. A scope flag composes with the existing `-s`/tag
  filters: `bl list --everywhere -s ready` is the mass-execution dispatch
  query.
- **`show` stays global.** Naming an id is the explicit signal; scoping reads
  by location would just make cross-repo edges illegible from either end.

**Confirmed in dialogue — the mental model IS the draft's:**

> "there is an implicit --repo=this-one (or whatever the syntax is) when issuing
> that command? which is overridable, of course, and the --everywhere (or
> whatever interface) flag is really just omitting the filter?"

Exactly so. The implicit predicate is not a new construct: it IS `claim`'s
admit test (this checkout's root, plus rootless balls), and `--everywhere`
simply omits that predicate. Everything else composes unchanged. The spelling
was delegated ("or whatever the syntax is"), so the draft's `--everywhere`
stands. It is **not** spelled `--repo=X` because there is nothing to name: the
scope input is computed from the invocation path, not typed. Viewing a specific
other repo's slice is `cd` into it — invocation path is already the routing
signal for create, claim, and delivery, so a per-op `--repo` would be a fourth
spelling of a signal the shell already carries.

**Fleet-view labels — render-time sugar, config-free (converged):**

> "Root hashes in the fleet view feel like somethign that we can shadow with
> real branch/repo names probably as pure render-time sugar. Doesn't appear in
> --json, just something that shows nicely when someone is looking at the task."

Settled subtraction-first: labels are **render-time only, never in `--json`**
(the machine reads roots; only the human view shadows them). The source of
names is config-free local derivation — this box's XDG state already holds one
clone entry per enrolled checkout path, so match a foreign ball's root against
each enrolled checkout's computed root and render that checkout's directory
basename. Repos this box never primed have no entry to match, so they fall back
to the short hash: zero config, nothing to drift, degrades honestly cross-box
(a name never appears where the box can't earn it). The owner-authored
`hash → label` legend from Q1's residue stays documented as the
if-the-basename-actually-hurts fallback — never authority, and not needed to
ship.

**Cost, priced:** the root read is `git rev-list --max-parents=0 HEAD` — a
full-history walk, deliberately scoped away from update/unclaim/close
(bl-9bee). Root-aware `list` re-adds it where it is now load-bearing, once per
invocation, and only lazily: a catalog containing zero rooted balls never
shells git at all, so task-only stores stay walk-free. Do NOT cache the root
in config/binding — the cache would drift at exactly the moment it matters (a
history rewrite), and the walk is once-per-command, not per-ball.

## Q3 — enrollment: promote `--center` from alias to verb-meaning

Today `--center` is a weak alias of `--remote` (`src/checkout_args.rs:92`,
fills-an-empty-slot) — two spellings of the same ephemeral override. bl-c2de
named the seam between the ephemeral tier and the durable tiers; give the two
spellings that exact split:

- **`--remote URL`** — unchanged: per-op override, shapes one invocation,
  persists nothing, every store-touching verb.
- **`bl prime --center URL`** — enrollment, prime-only, durable by
  definition. Sugar for the composition that already exists:
  1. `bl conf set task-remote URL` — the per-clone binding (bl-d081), the
     durable pointer that never travels on install;
  2. `prime --install URL` — adopt the center's committed `config/`
     (`src/adopt.rs`, single hop, §6 path-copy, sealed commit = the undo);
  3. ordinary prime — clone/sync the store, converge.

One command, from any satellite checkout, zero origin surgery, no global-XDG
trap, no half-enrolled window (binding set but config not adopted, or the
reverse — the state the cross-tracking trial kept hitting). Re-running is
prime-idempotent: the binding write converges, install converges, sync ffs.

**Is install's consent gate sufficient (the malicious-config worry)?** Yes,
and the one-shot does not widen it: the command names its source explicitly in
argv (nothing discovered, nothing transitive), the copy is the same visible
sealed commit, and the §6 rule holds — the schedule travels, `bin/` never
does, so a center still cannot make a box run a binary that wasn't installed
beside `bl` by hand. The trust act is identical to today's two-step; it is
merely spelled once. The maintainer's "visible install process from a trusted
source" IS the shipped §0/§6 design.

**Surface cleanup:** with `--center` promoted, keeping it as an ephemeral
alias on `sync`/mutate verbs would contradict its new meaning per-verb. Drop
the alias there (`--remote` remains); pre-1.0, recently added (bl-c2de), cheap
now and confusing later. The rule reads: *`--remote` shapes one op; `--center`
enrolls a checkout.* This drop is **settled by consequence** of the promoted
enrollment meaning — one flag meaning durable-here-but-ephemeral-there is worse
than two rules. The maintainer did not address it directly, so it stays
veto-able at implementation without disturbing the rest of the design.

**Routing new work:** `create` stamps the INVOKING checkout's root — so you
route a ball to repo B by creating it from repo B. No `--root <hash>` flag:
invocation path is already the routing input everywhere else (claim, delivery),
`root_commit` stays reserved/unforgeable (`src/mutate_build.rs:162`), and
nobody types hashes. cd is the existing signal.

## Q4 — stealth and local-only: the zero case, not a fork

A shared center is NOT inherently a network remote: the remote ladder takes
any git URL, including a filesystem path. Two repos on one box share a store
through a local bare repo — `git init --bare ~/hub.git`, then
`bl prime --center ~/hub.git` from each — no network, no forge, same code
path as a hosted center.

True stealth (`task-remote none`) means "this checkout's store has no home but
its own XDG clone" — sharing is definitionally out, because cross-repo work
requires the store to have ONE home two checkouts can reach. That is not a
design fork; it is federation's zero case (§12) applied to N projects: each
repo its own store, no cross edges, exactly today's behavior. The upgrade path
is the existing §12 re-home discipline (move the store, then the pointer) —
enrolling a formerly-stealth repo into a center is `bl install tasks/* --to`
the center's branch, then `prime --center`.

## Q5 — delivery placement audit: nothing assumes store-repo == code-repo

Confirmed by read:

- `create`/`claim` compute the root off the invocation path alone
  (`src/mutate.rs:75`); the guard compares ball-vs-invocation
  (`src/change.rs:107`). The store's location never enters.
- The worktree territory is keyed by the mirrored invocation path; the
  `work/<id>` branch forks the INVOKED repo's `main`; close's fold, gate
  (repo's own pre-commit hook), and squash all run against that repo.
- Each enrolled repo keeps its own XDG store clone of the shared branch
  (`clones/<percent-encoded-path>/tasks`); the center is the sync point.
  Contention across satellites is the existing E5 story — no new state.
- Reads never shelled git against the code repo before; root-aware `list`
  (Q2) is the one new touch, priced above.

## Edges, priced

- **Multi-root repos.** `rev-list --max-parents=0 HEAD` prints EVERY root;
  `root_commit()` takes the first line (`src/delivery_repo.rs:80`). Merging an
  unrelated history can reorder roots, flipping the computed identity and
  stranding earlier balls (root_commit is reserved — no repair edit). Fix
  worth taking with this ball: match the recorded root against the SET of
  current roots (any-of admits), a strictly-more-correct identity read, tiny
  diff, and it makes "vendored an unrelated history" a non-event.
- **A true root rewrite** still orphans the repo's balls (identity is the
  history; rewrite the history, forfeit the identity). Accepted: rare,
  deliberate, and the same event already orphans every work branch. Re-create.
- **Rootless legacy leak.** Rootless balls are visible+claimable everywhere by
  design (fail-open), so a satellite enrolling into an old center sees its
  pre-bl-1ce7 legacy balls as ready. Accepted noise: shrinks as legacy balls
  close, and stamping them retroactively would forge identity the seal never
  recorded. If it hurts in practice, the cure is closing or tagging them, not
  mechanism.
- **No atomic cross-repo delivery.** A spanning feature (API + client) is two
  balls, an edge, two deliveries — sequenced by `--needs`, never landed as one
  transaction. Git has no cross-repo atomicity to build on; pretending
  otherwise would be mechanism without a substrate. Scope boundary, stated.
- **`--center` alias removal** off non-prime verbs is a (pre-1.0) surface
  break. Priced above; the alternative — one flag meaning durable-here,
  ephemeral-there — is worse.

## Ledger

**Converged (ratified in maintainer dialogue 2026-07-04):**
1. No root→project roster in config — the stamps are the fleet (Q1). Ratified:
   "Config model derived from distinct roots: damn, that's way better. Aligned."
2. The escape hatch for two same-repo checkouts is already the primitive: the
   per-clone binding (bl-d081), orthogonal to root identity; no new override
   (Q1). Raised in dialogue and answered there.
3. `list` default = the claim-admitted set (this root + rootless); one flag for
   the fleet view; `show` stays global (Q2). Confirmed: the implicit predicate
   IS claim's admit test, and `--everywhere` just omits it; spelling delegated,
   so `--everywhere` stands.
4. Fleet-view labels are render-time only, never in `--json`, derived
   config-free from enrolled checkout basenames with a short-hash fallback; the
   owner `hash → label` legend is a documented last resort, never authority
   (Q2).
5. Enrollment = `bl prime --center URL` = binding + install + prime, one
   consent, prime-only durability (Q3).
6. Stealth is the zero case, not a fork; a local path is a legitimate center
   (Q4).
7. No `--root` on create — route by invoking from the target repo (Q3).
8. Root identity matches against the root SET, not the first line (Edges).

**Settled by consequence (veto-able at implementation):**
- Drop `--center` as an ephemeral alias on non-prime verbs — falls out of the
  promoted enrollment meaning (one flag meaning durable-here/ephemeral-there is
  worse); not addressed directly in dialogue (Q3).
