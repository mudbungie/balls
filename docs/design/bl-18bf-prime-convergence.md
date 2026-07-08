# bl-18bf — prime as the upgrade converger

**Status:** converged (attacked 2026-07-07; holds with the amendments noted inline).
**Deliverable:** this document; implementation rides four follow-on balls
(created at design close, sequenced with `--needs` edges, each citing this
record).

## The problem: version skew is not drift

`bl doctor` was built twice and deliberately burned (bl-77a7 spec, bl-a38e code)
under the **no-repair-verb** principle: every fix is an existing idempotent verb
(`prime`/`install`/`sync`/`update`), and drift fails loud at point of use. That
analysis was about **steady-state drift** and it still holds — nothing here
reintroduces a doctor verb.

What bl-77a7 never evaluated is **version skew**: a checkout that was healthy
under an old binary and is stale under a new one. The recorded history of the
substrate produced a catalog of exactly that:

- seeded schedules ≤ 0.5.3 name `tracker`, renamed to `bl-tracker` (bl-27bf) —
  today an eternal skip-with-notice whose fix is a hand edit;
- `stealth.lock`, written by an old `prime --stealth` and then retired as a
  concept (bl-9df0) — an operator who declared stealth with the old mechanism is
  **silently un-stealthed** by the modern remote ladder (nothing reads the file;
  origin is rediscovered and published to);
- `work/<id>` branch debris (52 had accumulated before bl-292d) and orphan
  `changes/<uuid>/` crash worktrees;
- a claimed ball whose code worktree was deleted out from under it — since
  bl-c2bf prime no longer re-materializes, and nothing says the paved path
  (unclaim + reclaim);
- the pre-0.5.0 legacy store (already handled: prime quarantines, §16 runbook).

Upgrade pain arrives as a **composition** of these, and point-of-use errors
surface them one trip at a time. The missing piece is not detection mechanism —
almost all of it exists — but one moment where the whole composition converges
or is named.

## The reframe: no new verb — prime already has this job

`prime` is already the idempotent converge-this-checkout op: it founds the
substrate, seeds and prunes the schedule, quarantines legacy stores, prunes
settled `work/*` branches. Upgrade convergence is **more of prime's existing
job**, not a new job. `bl conf` is already the read-side diagnostic (resolved
values + provenance layers + unbound rows with `[source]` hints, bl-5b09).

The owner's stated concern, recorded as a design constraint: prime is
high-throughput (every session start, hook chains), and this adds code surface
to it. The severability asymmetry is why it starts inside prime anyway — *it is
easier to break this behavior back out (lift the call sites into a verb) than to
fold a verb back in.* Everything below is structured for that extraction.

## Scope: three pieces

### 1. Rename convergence (core)

At prime, rewrite every occurrence of a retired first-party name
(`renames::renamed_to`, the closed static map — today `tracker` → `bl-tracker`)
in the **landing's** `config/plugins.toml`: `[hooks]` list entries and
`[source]` keys. One ordinary landing commit; no-change means no commit, so a
converged checkout re-primes to a no-op.

Write path, precisely: `conf_write::edit_landing_toml` (today private; goes
`pub(crate)`) with a raw-`toml::Table` rename closure over the `[hooks]` arrays
and `[source]` keys. **Not** `Hooks::to_toml`, which serializes only its own
two tables and would silently drop a team's foreign tables that the raw path
round-trips. Comments are lost — true of every conf write already. `conf_write`
has no rename op and no `[source]` surface; this closure is the one addition,
not a new `bl conf` verb surface.

Why this is not overreach on a committed schedule: the map is closed, static,
and first-party-reserved (`bl-` is reserved, §5/§6). A retired name has exactly
one meaning and can never bind again — the rewrite is spelling correction, not
policy change. The bl-27bf notice was explicitly a stopgap "until its owner
updates it"; prime acting as the owner's hand *is* the update. This also makes
`renames.rs` entries actually prunable: one release after the rewrite ships, no
live landing can still carry the old name, which is the map's documented
severance condition.

The rewrite must not make things *worse* than the notice it replaces: the old
name's `config/plugins/bin/<old>` symlink dangles, so a bare rewrite would turn
a non-fatal skip-with-notice into a hard "referenced but not installed" abort
on every plugin-dispatching verb. So convergence finishes the job by the
**seed's own rule**: bind the new name to the sibling binary beside `bl` when
present (first-party renames ship beside `bl` by construction); when absent,
the ordinary unbound abort with its `[source]` hint applies — same refusal the
user would meet on any fresh machine, and `bl install` remains the fix. The
dangling old-name symlink is gitignored local cruft; converge removes it with
the rewrite (this is the one deletion allowed: a *dangling* symlink is not
work).

**Live-binding guard.** The `bl-` reservation cuts both ways: `tracker` is not
a reserved name, so a third-party plugin legitimately named `tracker` with a
live `bin/tracker` binding may exist. Converge acts only when the old name is
*unbound* (`Registry::resolve_bin(old).is_none()` — canonicalize+is_file, the
same query `conf` uses): a live-bound old name means it is not our retired
plugin, so converge neither rewrites that name nor touches its symlink, and
dispatch invokes it as ever.

Ordering, precisely: converge runs **after** landing founding/rebind and
**before** the prime chain's `Hooks::effective` read — so the op that performs
the rewrite dispatches the rewritten schedule and resolves the fresh binding in
the same breath; one prime converges and resumes, no "run prime twice" step. On
a first prime there is no landing until mid-op and the embedded seed already
names `bl-tracker`, so converge no-ops. Note the rebind step that precedes it
binds the *committed* (old) names and finds nothing — converge's
rewrite-then-bind is one step for exactly this reason.

**Adopt paths converge too.** `--install`/`--center` copy a center's committed
`config/` into the landing; a center whose schedule still names `tracker` would
otherwise re-inject the old name on every adopt, making prime's rewrite a
commit per install+prime cycle instead of once ever. So `adopt` applies the
same closed rename map to what it copies in — semantics-preserving for the same
reason, and it closes the loop without requiring the center to have converged
first.

Boundaries:
- **Landing only.** An old name in the XDG layer (`config.toml` hooks) is the
  user's file; prime does not edit it. The dispatch-time rename notice
  (`plugin.rs`) survives unchanged as the point-of-use cover for that layer and
  for un-primed checkouts.
- Federation: the landing is per-checkout, single-owner, install-transport
  (§2). Each clone converges its own landing on its own prime; config crosses
  checkouts only by `install`, unchanged.

### 2. Debris report (report-only; prime never deletes what may hold work)

Emitted through the op log at `info` + stderr echo — the bl-b1be idiom already
used for seed prune notes and install's dangling report. Each line names the
fixing command. Prime **reports and refuses**; deletion stays a human/agent act.

Core-side (one `readdir`, one `exists`):
- **Orphan `changes/<uuid>/`** under the clone dir: crash debris from an op
  whose teardown never ran. Report with `git worktree remove <path>` (may hold
  uncommitted work; racy-safe because it is a report, not a delete).
- **`stealth.lock` present but stealth undeclared**: the retired file exists
  while the durable ladder resolves a remote. This is the one *silent-publish*
  hazard in the catalog, so it warns loudest: "stealth.lock is retired and
  unread — declare stealth with `bl conf set task-remote none`, then delete the
  file." Suppressed when the landing sentinel already reads `none` (the
  operator re-declared; the file is inert cruft, still named for deletion).

Delivery-side (`prime.post`, beside the existing bl-292d prune, which already
enumerates `work/*` and computes Standing) — **one** report, computable by the
plugin alone:
- **Unsettled `work/<id>` whose worktree directory is absent**:
  committed-but-undelivered content with nothing checked out on it. Report
  names both remedies — `bl claim <id>` (re-materializes onto the surviving
  branch; the bl-65e0 contract that a later claim-and-close delivers it) or
  discard with `git branch -D work/<id>` — never pruned, now *said* instead of
  silent.

An earlier draft also wanted "claimed ball, missing worktree." Dropped: the
claim set is store data absent from the §7 prime payload, and the worktree is
plugin territory core cannot stat — producing that report requires widening the
wire or breaking the plugin's kind-blind/stateless contract, both out of
proportion. The report above covers the same debris from the side that can see
it (branch present, dir absent), without asserting claim state.

### 3. The front door (docs)

- `SKILL.md`, one line in the invariants/flow area: *upgraded and things are
  weird? `bl prime`, then read `bl conf`.*
- `docs/architecture.md` §12: enumerate prime's convergence duties (rename
  rewrite, debris report) and record the severability clause below. §15
  revision-log entry pointing here.

## Cost budget (the high-throughput constraint, made checkable)

On a **converged checkout** (the overwhelmingly common prime):
- rename check: one extra read+parse of the landing `plugins.toml` (the prime
  chain parses it later and separately), names scanned against a static map —
  no git, and only when the old name is present-and-unbound does any write
  happen.
- debris checks: one `readdir` of `changes/`, one `exists()` for
  `stealth.lock`, and — delivery-side — zero new subprocess spawns (the
  `for-each-ref` and Standing computation already run for the bl-292d prune;
  the worktree-dir probe is one `exists()` per unsettled branch).

**Zero new subprocess spawns and zero new commits on the clean path.** Git work
(the landing rewrite commit) happens only when a retired name is actually
present — once per checkout on the plain path, and adopt paths stay converged
because `adopt` rewrites what it copies in. Any implementation that spawns a
process on the clean path is out of contract.

## Severability (the extraction contract)

All convergence logic lands behind **one core module boundary, `src/converge.rs`**
(rename rewrite + core-side debris report; decomposed to sibling modules with
re-exports if the 300-line cap demands, per repo convention), called from
prime's flow at exactly one site; the delivery-side report sits beside
`delivery_prune.rs` with the same one-call-site shape. Breaking it back out — if prime grows too heavy — is
moving those call sites under a new verb, not a rewrite. The module owns no
state and persists nothing (reports are log lines; the rewrite is an ordinary
landing commit), so extraction has no migration of its own.

## What prime still refuses (unchanged lines)

- **Never executes `[source]` hints.** Acquisition stays human-driven; the
  explicit `-y`/`--trust` surface is bl-5b09-deferred pending provenance design.
  Convergence reports name commands; it fetches nothing.
- **Never deletes** debris, worktrees, or unsettled branches.
- **Never adopts** a legacy store tip (bl-868d quarantine, §16 runbook) or
  foreign config (`install`'s consent-gated job).
- **Never edits** XDG-layer config.

## Consciously excluded

- A `doctor`/`check`/`--dry-run` verb or flag: the report already *is* the
  dry-run for everything prime refuses to touch; the only mutation (rename
  rewrite) is semantics-preserving by construction. Adding a read-only twin
  re-litigates bl-77a7.
- Auto-pruning unsettled branches or change worktrees with an age threshold:
  a threshold is a config knob plus a data-loss path; a report line is neither.
- Pre-XDG-era (pre-0.5.0-clone) recovery: those installations predate the
  greenfield model entirely; the answer remains re-init + §16 import, a runbook
  not a mechanism.

## Attack record (what was raised, what changed)

An adversarial source-level pass (2026-07-07) against prime's control flow,
the conf write path, seed/registry binding, the stealth.lock history, and the
delivery wire produced:

- **BROKE "claimed ball, missing worktree"** — the §7 prime payload carries no
  claim set and core cannot stat plugin territory. Replaced with the
  branch-present/dir-absent report the plugin computes alone (above).
- **"Same write path bl conf uses" was overstated** — `conf_write` has no
  rename op and no `[source]` surface, and `Hooks::to_toml` drops foreign
  tables. Fixed: `edit_landing_toml` goes `pub(crate)` with a raw-table rename
  closure (above).
- **Unconditional old-symlink delete was unsafe** — a third-party plugin
  legitimately named `tracker` could be live-bound. Fixed: the live-binding
  guard (above); converge acts only on an unbound old name.
- **"Once per stale checkout, ever" was false on adopt paths** — a stale
  center's config re-injects the old name each `--install`/`--center`. Fixed:
  `adopt` applies the rename map to what it copies in.
- **"Top of prime" was imprecise** — the safe window is after
  founding/rebind, before the prime chain's hook resolution; and rebind binds
  the committed (old) names, which is why converge rewrites-then-binds as one
  step. Fixed in the ordering paragraph.
- *"stealth.lock check is dead code for a file nobody has"* — kept, narrowly:
  the file's location (`clone_dir` root, core-owned) and the absence of any
  lock reader in the current remote ladder were both confirmed in source; it
  is the catalog's only silent-*publish* hazard, one `exists()` on the clean
  path, self-retiring. Known softness: core cannot see whether a remote
  actually resolves, so the warning may fire for a checkout that is
  circumstantially stealth anyway — the wording claims no publish, only that
  the mechanism is retired.
- *"Rewriting a committed schedule is policy overreach"* — held for the
  landing only, on the closed-map/one-meaning argument; the XDG layer stays
  untouched, which is also the severability line.
- *"prime is high-throughput"* — held via the checkable clean-path budget
  (one extra file read; zero new spawns, zero new commits).
- *"whose landing, under federation"* / concurrent primes — held; per-checkout
  single-owner landing, and non-atomicity is the same class as existing
  conf writes on one checkout.

## Implementation plan (follow-on balls)

1. **Core rename convergence** — `src/converge.rs`: rewrite retired names in
   the landing `plugins.toml` via `edit_landing_toml` (made `pub(crate)`),
   rewrite-then-bind in one step, live-binding guard, dangling-symlink
   removal; wire into prime after founding/rebind, before the prime chain's
   hook resolution; apply the same map in `adopt`; the dispatch notice
   unchanged. Tests: idempotence (second prime = no commit), `[hooks]` +
   `[source]` both rewritten, foreign tables round-trip, live-bound old name
   untouched, first-prime no-op, adopt-path rewrite, XDG layer untouched.
2. **Core debris report** — same module boundary: `changes/` orphans +
   `stealth.lock` hazard, through the op log; suppression when the landing
   sentinel declares stealth. Tests assert report lines by value (bl-b1be
   style) and clean-path silence.
3. **Delivery-side report** — beside `delivery_prune.rs`: unsettled `work/*`
   branch with absent worktree dir. No new spawns (reuse the prune's
   enumeration and Standing). Tests via the existing delivery fixtures.
4. **Docs** — SKILL.md front-door line; architecture §12/§15 amendments citing
   this record.
