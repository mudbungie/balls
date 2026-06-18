# bl-chore — the guarded close-gate mint at claim (bl-3df3)

**CONVERGED 2026-06-18 (maintainer dialogue): Option A, no-op rollback, title +
optional body/priority.** The three open questions below are all resolved in
favour of the recommendation — the maintainer blessed the reversal of the epic's
free-form lean (A over B), the rollback deviation from §11 (no-op), and the
optional `body`/`priority` fields. This file is the authoritative reasoning
record; the spec text the bl-759f doc-update lands (§6/§10/§11) is the
authoritative *behaviour*.

Design record for `bl-chore`, the opt-in first-party plugin that mints tagged
close-gate children at `claim.post`. Amends `docs/architecture.md` §6/§10/§11
(frozen §0–§17, so the edits are deliberate amendments the bl-759f doc-update
child lands — enumerated under *Spec touch-points*); the living spec text there
is authoritative, this file holds the decision reasoning at full length.

## What it is

At `claim.post`, for the just-claimed task, mint one close-gate child per
configured chore ("Run the test suite", "Review and update the docs") so the
claiming agent must discharge them before `bl close` succeeds. **Create-side
only** — bl-chore never *resolves* a gate; a human (or an orthogonal resolver
plugin) closes it. A gate child is `bl create --parent <id> --blocks close` (§10):
it does not affect readiness and never shows as a status, so N parallel claims
cause zero ready-list clutter — the chores surface exactly where wanted, at
`bl close`, "blocked by: run the test suite."

## Two corrections to the epic's framing (bl-6ee9)

The epic is mostly right, but two load-bearing claims are false against current
code and the design is cleaner once corrected (CLAUDE.md: *amend and correct
previous conclusions; fix the doc, don't deviate silently*).

1. **bl-chore is the FIRST guarded mint, not a refactor of forge's.** The epic
   says it "factors forge's hand-rolled `claim.post` mint into one place."
   Verified false: the shipped forge plugin (`~/dev/balls-github-plugin`) has
   **no `claim.post` mint** — only `push` (open PR) + `sync` (close the gate on
   merge). Its README and code assume a core `bl review` op/`review` status that
   were **abolished** (`architecture.md:702` "There is no `review` verb"; `:242`
   "abolished `review`"). §11 still *specs* forge minting at claim, but nobody
   built it; the forge plugin is in fact stale (`push.rs`/`sync.rs` gate on
   `status=="review"`). So there is no existing mint to factor — bl-chore is the
   first implementation, which makes it a *cleaner* subtraction, not a refactor.

2. **Demote "the primitive other plugins sit on."** Across the binary/repo
   boundary, forge (a separate binary) cannot call bl-chore's `claim.post`
   handler, so it does not "sit on" bl-chore *through config*. What it can reuse
   is the **mint code** — a `mint_gate(invocation_path, parent, title, body,
   priority, tag)` seam that takes a *tag parameter* (forge passes `forge-review`,
   not `bl-chore`) — or, lacking a shared crate, the *pattern* (the two guards +
   safe render), copied. bl-chore earns core-shipping as **the reference
   chore-fanout every repo wants**, not as a library everyone links. The
   config surface stays titles-only; reuse is a code seam, not a config API.

## The config schema — lead with subtraction

The central tension the epic names ("inject into arbitrary shell" vs
"declarative child-specs"). The safe mint rendering is
`bl create <flags> -- "<title>"`: getopt's `--` end-of-options separator forces
every flag *before* `--` with the title as the lone trailing positional — which
is **only constructable if bl-chore owns the whole argv.** That single fact
selects the schema.

### A. Declarative chore specs — data, not shell (RECOMMENDED)

Config is a bare list of specs in the plugin's own territory
(`config/plugins/bl-chore/chores.toml`), each a **title** (required) plus
optional declared **body**/**priority** — never flags, never shell:

```toml
[[chore]]
title = "Run the test suite"
[[chore]]
title = "Review and update the docs"
body  = "Check §6/§11 register the capability; update README."
# epic-skip default-on: a leaf task you gave one real subtask gets NO chores.
```

bl-chore renders each spec into exactly one command, executed via **argv (no
shell)** with `cwd = binding.invocation_path`:

```
bl create --parent <bl-id> --blocks close -t bl-chore [-p N] [--body B] -- "<title>"
```

- **Dissolves flag-injection** — there is nothing to inject *into*; bl-chore
  builds the command, puts its flags before `--`, and passes the title as one
  literal argv positional (a body with quotes/`$()`/newlines is inert — never
  interpolated, because there is no shell).
- **Closes the recursion hole structurally** — the caller supplies only a
  title and *cannot* forget (or override) `-t bl-chore`; the plugin always
  injects it. The one interface invariant — "minted children carry the tag" —
  holds by construction, not by discipline.
- **Makes the guards meaningful** — they guard a known mint shape.

*What A gives up:* expressiveness. Answer: `title` + optional `body`/`priority`
covers every real chore (the body is the forcing-function checklist — *how* to
discharge it). A genuinely richer gate (a `--needs` cross-edge, a forge-shaped
gate) is a **code seam** (`mint_gate` with a tag param) or a **separate
plugin** — not a config knob. A is narrow on purpose.

### B. Free-form `bl` command lines (the epic's stated lean) — REJECTED

Config is a list of free-form `bl` lines, "do whatever you want"; bl-chore
appends `--blocks close`/`-t bl-chore`.

*What doesn't this solve:*
- **It adds nothing over a capability core already ships.** "Run an arbitrary
  command at `claim.post`" is *any* plugin in the `claim.post` hook list — a
  plugin is already arbitrary local code (§6). A free-form bl-chore is a
  redundant re-implementation of the schedule one level down; it forfeits the
  only thing a *named first-party* plugin is for.
- **It breaks the gate contract and the recursion break.** You cannot safely
  splice `--blocks close`/`-t bl-chore` into a line that may already contain
  `--`, quoting, a pipe, or its own `-t` — the injected flags land *after* `--`
  as positionals (a child titled `--blocks`, or a parse error), and a
  non-`bl-create` line (`gh issue create`) is a fire-and-forget that gates
  nothing while the operator *thinks* they configured a gate. The tag is no
  longer guaranteed → tag-skip is defeated → a claimed chore mints a
  grandchild, and post-`bl-7110` a deep enough cascade **aborts the claim** at
  the depth-8 cap rather than misbehaving quietly.

The real needs B points at (open an issue at claim, tag a sibling) are *real* —
and are each a narrow `claim.post` plugin, not a reason to widen bl-chore into
an unsafe everything-runner.

### C. A + arbitrary structured extra flags — DEFERRED

A, plus per-spec passthrough of extra `bl create` flags.

*What doesn't this solve:* extra flags could include `-t`/`--blocks`/
`--subtask-of`/`--parent` and reopen both holes A closes; it needs a ban-list to
stay sound. A's three data fields already cover real chores. Earn C later with a
concrete chore A can't express — don't pre-build the passthrough (config knobs
earn their keep).

## The two guards (both plugin-side — §10 keeps core mint-free)

Core *enforces* blocking but **never mints edges** (§10's auto-mint rejection);
*creation* is left to plugins. A plugin minting an explicit `bl create --blocks
close` is exactly the "creation is left to plugins" side §10 reserves — the
boundary is respected. Both guards are creation-side policy, so both live in
bl-chore, never core. **They are not peers:**

- **TAG-SKIP — always-on, never a knob, structural.** On `claim.post`, if the
  claimed task's tags contain `bl-chore`, exit 0. Reads `previous_state.tags`
  *straight off the §7 post wire* — no store query. Load-bearing because **a
  chore is a leaf**: it has no children, so the epic-skip has-children check
  would *not* catch a claim of a chore. This is the recursion break, and it only
  holds because A guarantees the tag on every minted child.
- **EPIC-SKIP — a knob, default-on, in the plugin's OWN config.** If the claimed
  task already has any *live* child, exit 0 — keeps epics clutter-free and buys
  **idempotency for free** (after firing once the parent has chore children, so
  a reclaim re-hits epic-skip and won't dup). Children are **not on the wire**
  (emergent via others' `parent` pointers), so this is the one **store query**:
  `bl list --json` (cwd=invocation_path) filtered `parent == metadata["bl-id"][0]`,
  which sees only live children. The knob lives in `config/plugins/bl-chore/`
  (balls never reads it — severability), adding zero core surface.

Check tag-skip first (off-wire, cheap) before spending the epic-skip query.

## Rollback — none, a stated deviation from §11

bl-chore ships a **no-op rollback** (handles `claim post` with `rolling_back`
set, exits 0). Each `bl create` bl-chore shells seals+pushes independently via
its *own* `create.post` — so a minted gate is **published state the moment it
returns**, not the claim op's private derived state. On a `claim.post` abort the
gates correctly persist to gate whoever next holds the task, and a reclaim is
de-duped by epic-skip → no dup. §14 blesses exactly this: "persistence-through-
abort is a plugin choosing a no-op rollback, not a core carve-out"; and a
teardown rollback would have to shell N `bl close` ops (each a seal+push that can
fail or hit the cap — rollback can't spawn at the cap), turning a tidy into a
recursive transaction.

This **deviates** from §11:1115's forge parenthetical "(forge: also remove the
just-minted gate child)." The deviation is correct *for bl-chore* because
forge's gate is a single *unpushed* edge while bl-chore's are independently
sealed — so the doc-update amends §11 to record the distinction rather than
inherit forge's teardown.

## Wiring & order

Opt-in: **NOT** in the default `[hooks]` schedule (default-wiring would mint
chores for every claim, system-wide). Enable with
`bl conf prepend claim.post bl-chore` — **prepend** puts it at the list head,
*before* bl-tracker, so tracker's push stays the single outermost irreversible
act and a bl-chore abort is fully local-reversible. Appending *after* tracker is
the non-ff footgun (tracker has already published the claim seal; an un-seal
then diverges from the remote). Depth is safe: bl-chore is wired only on
`claim.post` and shells op=`create`, so it never re-triggers its own op; the N
chores are siblings at depth-1, not nested (the spec's worked bounded case,
§6:581).

## The epic's attack list — answered

- **Does free-form shell break tag injection?** Yes — the decisive argument for
  A over B (the `--` splice, above).
- **unclaim → reclaim?** epic-skip catches it (live chores still parent the
  task). If *all* prior chores were closed then the task unclaimed-not-closed,
  reclaim re-mints — correct: an ungated task's next holder *should* be re-gated.
  Some-closed: the survivors suppress re-mint, no partial dup.
- **A chore being claimed re-fires?** tag-skip catches it (the chore carries the
  `bl-chore` tag); epic-skip can't (a chore is a leaf).
- **Third-party plugin that forgets the tag recurses?** Impossible from config
  under A (the caller supplies only a title; the plugin injects the tag). A
  plugin minting an *untagged* chore-shaped child via its own `bl create` is on
  its own — which is why owning the argv is the safe boundary.
- **Two chores, same title?** Two distinct gate children (ids differ; titles
  aren't keys) — harmless, the operator dedups their own list. Not bl-chore's
  job.

## Where it ships & spec touch-points (for bl-759f)

First-party `bl-chore` binary + library in the core repo, structured like the
adversary plugin (thin `src/bin/bl-chore.rs` edge: handle `protocol` via a const,
read env once at the edge, read the §7 wire from stdin, op/phase from argv,
delegate to a `balls::chore` lib behind a trait seam so the branch matrix hits
100% coverage without a temp repo). 100%-coverage + 300-line-per-file constraints
apply; lift to sibling modules + `_tests.rs` sidecars as it grows. Makefile:
`install-chore` mirroring the `bl-delivery` target (not the stale `tracker` one).

The bl-759f doc-update amends, in order:
- **§6:426-428 prose** — add `bl-chore` to the shipped-capabilities sentence
  (and fix the stale `tracker` → `bl-tracker` there), noting it is opt-in.
- **§6:497-508 `[hooks]` block** — do **not** add a line; add a one-line comment
  "bl-chore ships but is not wired — opt in via `bl conf prepend claim.post
  bl-chore`" (the load-bearing *shipped ≠ scheduled* distinction).
- **§10:982-986 ("Gates are tasks only")** — primary home: 1-2 sentences naming
  bl-chore as the create-side guarded-mint primitive + its two guards + the
  core-never-mints reassurance + the *resolution-is-a-separate-plugin* boundary
  (block the "just have bl-chore also run `make test`" scope creep).
- **§11:1085-1096 (forge bullet)** — reframe forge as `bl-chore` (create) +
  forge-`sync` (resolve), state forge sits on the mint *with its own tag*, and
  leave a breadcrumb that the shipped forge plugin is stale (abolished
  `review`). Note bl-chore ships no rollback, so §11:1115's teardown is not
  inherited.

## Recommendation & net mechanism

**Ship Option A.** Two guards (tag-skip always-on, epic-skip default-on knob in
plugin territory). No rollback handler. Opt-in via `bl conf prepend claim.post
bl-chore`, before bl-tracker. bl-chore is the create-side reference capability;
resolution is orthogonal (a separate plugin), said explicitly to block scope
creep. Net mechanism: **+1 first-party binary, +1 tag convention (`bl-chore`),
0 new core config surface, 0 new flags, 0 new verbs, 4 spec amendments**, and
−1 false framing (the "factors forge's mint" / "primitive others sit on" claims,
both corrected).

## Open questions — RESOLVED (maintainer dialogue, 2026-06-18)

1. **Config schema: A vs the epic's free-form lean (B). → A.** The maintainer
   blessed the reversal of the epic's "do whatever you want" lean; B's
   non-`bl-create` side effects at claim are a separate narrow plugin, not
   bl-chore.
2. **Rollback deviation from §11. → no-op.** The doc-update amends §11 to
   distinguish an independently-sealed mint from forge's unpushed gate; no
   teardown handler ships.
3. **Optional `body`/`priority` spec fields. → keep.** The body is the
   forcing-function checklist; priority lets a release-blocker outrank a nit.
   Still data-only — zero injection surface.
