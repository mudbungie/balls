# Minimalism review — does used mechanism earn its keep?

**Ball:** bl-004c (parent epic bl-72a8) · **Reviewer:** Pare · **Date:** 2026-06-09
**Founding principle (§0):** *"Subtract before adding. A new verb/state/field/flag
is a smell; prefer an existing signal. Derive values rather than store them; make a
component indifferent rather than teach it cases."*

**Scope.** This review questions whether **used** mechanism should exist at all —
distinct from the dead-code audit (bl-ff89, removes *unused* mechanism) and the
deps line-item in standards (bl-5cf3). For every concept / verb / flag / config
field / abstraction / module seam it asks: does it earn its keep, or is it the
general path with empty inputs (a special case that's a missing reframe)? Can two
representations of one fact collapse to a single source of truth? Is each interface
deep-and-narrow? It then audits the dependency tree.

**Method.** Read every `src/*.rs` module + `docs/architecture.md` §0–§17. Cross-checked
each candidate's live callers with `grep`. Ran `cargo build`/`cargo clippy
--all-targets` (zero dead-code/unused warnings — so nothing here is *dead*, it is
all *reachable*; the question is whether it should be), `cargo tree`, and
`cargo machete`.

## Verdict

**The core is already exceptionally minimal and disciplined.** The single-source-of-truth
discipline is real and load-bearing: id-is-path (no `id` field), no stored `status`
(derived ladder), occupancy = one `claimant` field, blockers as the lone relational
primitive with `On = Verb` (one type, no claim/close special case), `delivered_in`
as a query not a field, and one wall-clock read site. Clippy/compiler confirm there
is **no dead code** to remove. Consequently the **safe applied cuts are few** — the
elegance is already paid for.

The genuine minimalism findings are **design-significant**: they are places where the
*implementation carries spec-frozen-but-empty mechanism* — a struct field that is
always empty, a merge facility with no consumer, a skeleton type whose only live job
is a placeholder. Removing them changes a frozen-spec contract (§4/§7) or overlaps the
rewrite-skeleton remit, so they are **proposed**, not applied — left for the maintainer.

| # | Kind | Target | One-line | Locus |
|---|------|--------|----------|-------|
| **Applied** |
| A1 | dep doc | `getrandom` demarcation | document why the one C-touching dep is *not* std-replaceable (footprint rule) | `Cargo.toml` |
| **Proposed** (do NOT apply — design-significant) |
| P1 | wire field | `Command.field_changes` + `FieldChange` | a wire field core *never populates* — always `Vec::new()`; the diff's single source of truth is the worktree | `src/wire.rs:37-56`, `src/mutate.rs:150` |
| P2 | config facility | list-compose merge (`_prepend`/`_append`/`_ban`) | ~50 lines of list-merge serving **zero** list fields; `EffectiveConfig` is two scalars and projection drops the rest | `src/config.rs:123-177` |
| P3 | skeleton type | `Op` / `Op::plan` / `Op::phases` / `MUTATING_PHASES`/`DIFFLESS_PHASES` | a phase-shape type whose only live caller is the placeholder arm for one unwired verb; real lifecycle hardcodes phases | `src/op.rs:39-71`, `src/lib.rs:183-185` |
| P4 | empty-input flag | `Verb::Install` dispatch arm | the unwired `install` verb is the *sole* input to the `other =>` placeholder that keeps P3 alive | `src/lib.rs:182-186` |
| P5 | duplicate encoding | `Occupancy::claim/unclaim`, `Retire::close/drop` | test-only constructors that re-encode verb→claimant a second time (prod inlines it) | `src/change.rs:122-132,249-259` |

---

## Applied cuts

### A1 — Demarcate `getrandom` as a deliberate, non-substitutable dependency

`getrandom` is the only runtime dep that reaches a C library (`libc`), and the
minimalism brief explicitly asks whether "a few lines of std" could replace each
crate. It is used at exactly **two** sites — `IdScheme::generate` (`src/id.rs:62-68`)
and the change-worktree token (`src/mutate.rs:236-238`, which reuses the same
`IdScheme`). It is tempting to cut.

**Correct-substitute test (footprint rule, `feedback_footprint_demarcation`): it FAILS.**
The standard library exposes **no** stable, portable entropy source. A std-only
substitute (hashing a `SystemTime`/PID/address) is *not equivalent* — it is
predictable and collision-prone, and id minting uses rejection sampling precisely to
be unbiased. So the dep stays; the principle is *demarcation over removal*. The cut
applied is **documentation**, not removal: the `Cargo.toml` comment now records that
`getrandom` is the entropy primitive both id minting and the worktree token draw from,
and that no std equivalent exists — so a future reader does not re-litigate it.

`cargo machete` reports **no unused dependencies**, and `cargo tree` shows a lean
graph: `serde`/`serde_json`/`toml` (one serialization stack, §3/§4) + `getrandom`.
None is a candidate for std replacement. **No dependency is removed.**

---

## Proposed subtractions (un-applied — for the maintainer)

### P1 — `Command.field_changes` is a wire field that is always empty

`src/wire.rs:46-56` defines `Command { op, field_changes: Vec<FieldChange>, body_change }`,
and `FieldChange { field, value }` (`:37-44`). But the **only** core construction site,
`src/mutate.rs:149-151`, hard-codes:

```rust
Command { op: verb.token().to_string(), field_changes: Vec::new(), body_change: flags.body.clone() }
```

— and its own doc-comment (`src/mutate.rs:145-148`) states the reason verbatim:
> "Field-level changes are NOT duplicated here (single source of truth): a plugin
> reads them from the change worktree / the `before`/`after` states, not a second
> diff description."

`grep` confirms `FieldChange` is constructed **nowhere** outside tests, and
`mutate_tests.rs:175` asserts `field_changes.is_empty()`. So this is precisely a
*second representation of one fact* — the very thing §0 forbids: the diff already lives
in the staged worktree and in `current_state`/`previous_state`, and §7 says a plugin
"contributes by EDITING THE CHANGE WORKTREE … never by printing values back." The
`field_changes` field is the empty-input general path: it is plumbed, `serde`-skipped
when empty (i.e. *always* skipped), and never read.

- **Why it can go:** it is a wire slot for information that the design deliberately
  refuses to put on the wire (single source of truth is the worktree).
- **Replaces with:** nothing — `Command` collapses to `{ op, body_change }`, and
  `FieldChange` is deleted. Plugins already read field diffs from the states/worktree.
- **Risk/effort:** **design-significant.** §7 (frozen) names `command` as "`op` +
  intended `field_changes` + `body_change`", so this is a spec amendment, not a code
  cleanup — it needs a §15 revision-log entry. Effort low (delete a struct + one field
  + two tests); risk is the contract change. **Coordinates with bl-1a66 finding m1**
  (post-wire `current_state` never populated) — both are §7 wire slots the impl leaves
  empty; decide them together.

### P2 — The §4 list-compose merge has no list field to compose

`src/config.rs:123-177` implements the full §4 list-merge facility: `ListOp`
(`Prepend`/`Append`/`Ban`), `list_directive`, `compose_list`, `as_array` — about 50
lines. It composes `<field>_prepend`/`_append`/`_ban` directives into list-valued
config fields.

But `EffectiveConfig` (`src/config.rs:39-54`) has **exactly two fields, both scalars**:
`tasks_branch: String` and `log_level: String`. There is **no list field in core
config at all.** And `resolve` (`:80-91`) projects the merged table onto
`EffectiveConfig` via `try_into()`, which **drops every unknown key** (serde). So a
composed list — even if a plugin's config supplied one — is built up in the merged
`Table` and then *discarded* at projection. The facility's output reaches no consumer.

This is the general-path-with-empty-inputs smell at config scale: a list-merge engine
running over zero lists. (Note: `feedback_config_knobs_earn_their_keep` and the §15
hooks-layering gap bl-1a66/M4 are adjacent — the `[hooks]` list, the one place lists
*would* matter, does **not** even route through this merge.)

- **Why it can go:** no core config field is a list; scalar replacement (the
  `None => base.insert` arm at `:127-129`) is all the two real fields ever exercise.
- **Replaces with:** plain scalar layering — `layer_over` keeps only the
  `base.insert(key, value)` branch; `ListOp`/`list_directive`/`compose_list`/`as_array`
  delete.
- **Risk/effort:** **design-significant.** §4 (frozen) specifies the compose directives
  as merge semantics, so this is a spec question: *is core config ever meant to grow a
  list field?* If yes (e.g. folding `[hooks]` into the layered merge — see bl-1a66/M4),
  the facility earns its keep the moment that lands and should **stay**. If `[hooks]`
  keeps its own un-layered path, the directives are speculative generality. **Decide
  jointly with bl-1a66/M4**; do not cut in isolation.

### P3 — `Op` is a skeleton type kept alive only by a placeholder

`src/op.rs:39-71` defines `Op { verb }` with `phases()` (returning `MUTATING_PHASES`
/`DIFFLESS_PHASES`) and `plan()` (a `"verb: author -> pre -> seal -> …"` string). The
module header still calls itself "this skeleton" / "the work inside each phase is the
seam each rewrite phase fills in."

The rewrite filled those seams **elsewhere**: the real lifecycle engines
(`src/lifecycle.rs`, `src/lifecycle_diffless.rs`) drive `Phase::Pre`/`Seal`/`Post`
**inline** and never call `Op::phases()`. `grep` shows `Op::new`/`.plan()`/`.phases()`
have **one** production caller between them: `src/lib.rs:184`, the `other =>` arm:

```rust
other => { println!("{}", Op::new(other).plan()); Ok(()) }
```

So the entire `Op` type, both phase-array constants, and `Phase::token` exist to print
a one-line plan for verbs that fall through to the placeholder. (`Phase` itself and
`Verb::class()` are **not** in scope — they are used widely by the real lifecycle/log/
dispatch; only the `Op` wrapper + `plan`/`phases` + the two `*_PHASES` arrays + `Phase::token`
are placeholder-only.)

- **Why it can go:** it is leftover rewrite scaffolding (§8 skeleton) whose live role is
  a stub. The phase sequence is already authoritative inside the engines.
- **Replaces with:** delete `Op`, `plan`, `phases`, `MUTATING_PHASES`/`DIFFLESS_PHASES`,
  `Phase::token`, once P4 removes the only caller.
- **Risk/effort:** moderate; **overlaps bl-ff89** (dead-by-rewrite mechanism) — flagged
  here because the trigger is a *used-but-vestigial* placeholder, not strictly-dead code.
  Gated on P4. Effort: delete ~35 lines + their tests.

### P4 — The unwired `install` verb is the sole input to the placeholder arm

P3 survives only because `src/lib.rs:182-186` has a catch-all `other =>` dispatch arm,
and **exactly one** verb reaches it: `Verb::Install`. `prime`/`sync` route to
`checkout`, the read verbs to `reads`, the six mutating verbs to `mutate` — leaving
`install` (specced in §9, but unwired in core dispatch; it runs today only inside
`prime`/`adopt`, per `src/adopt.rs`) as the lone fall-through, where it prints its
op-plan instead of doing anything.

This is "a special case that's really a missing reframe": the placeholder arm is the
general path with one empty input. Either wire `install` to its real handler (closing
bl-1a66/M3, which separately flags that `install` bypasses the engine seal/rollback
spine), at which point the `other =>` arm and all of P3 vanish; **or** make the missing
arm an explicit usage error. Either way the placeholder dissolves.

- **Risk/effort:** **design-significant** — it is really "finish wiring `install`,"
  which is its own ball (bl-1a66/M3). Listed here because the *minimalism* consequence
  is that one unfinished verb is propping up a whole skeleton type (P3).

### P5 — Test-only constructors that re-encode verb→claimant

`Occupancy::claim`/`unclaim` (`src/change.rs:122-132`) and `Retire::close`/`drop`
(`:249-259`) are named constructors used **only in `change_tests.rs`** (`grep`
confirms zero production callers — `mutate.rs:113-136` builds both structs with literal
field syntax). `Occupancy::claim` re-encodes the verb→claimant rule
(`claimant: Some(actor)`) that production already expresses at `mutate.rs:117`
(`(verb == Verb::Claim).then(|| actor.clone())`) — two homes for one fact.

This one is **deliberately left un-applied and rated LOW-value.** Inlining the struct
literals into the tests would make them *more* verbose and couple them to field order;
the constructors are arguably a clean deep-narrow interface. Recorded only so the
duplicate verb→claimant encoding is on the record — **recommend: leave as-is** unless a
future refactor makes the two encodings drift.

---

## Confirmed already-minimal (no action — evidence the discipline holds)

These were examined and **earn their keep**; listed so a later reviewer need not re-walk them:

- **`Verb` doubles as a blocker's `On`** (`src/task.rs:65`, `On = Verb`) — one type, no
  claim/close enum special case. Textbook reframe.
- **No `status`/`state` field** — derived 3-state ladder (`Task::status`,
  `src/task.rs:120-132`); `ready`/`closeable` are named single-caller predicates of it,
  each justified as the §10 enforcement question.
- **One wall-clock site** (`mutate.rs:228-231`), injected into pure `BaseChange`s.
- **`encoding::percent_encode`** — single-pass, std-only, no regex; the one naming primitive.
- **`message.rs`** — delegates the trailer grammar to `git interpret-trailers`; no
  hand-rolled parser. Deep and narrow.
- **`civil.rs`** — Hinnant's ~25-line civil-from-days is exactly why there is no `chrono`
  dep; display-only. Correct-substitute already chosen in balls' favour.
- **`Payload`** — one struct renders pre/post/rollback shapes via `serde` skip-if-None,
  not three types (`src/wire.rs:96-118`).
- **`change_token` reuses `IdScheme`** (`mutate.rs:236`) — no second randomness primitive.
- **Dependency graph** — `cargo machete` clean; `serde`+`toml`+`serde_json` are one
  serialization stack (§3/§4); no crate is std-replaceable (see A1).
