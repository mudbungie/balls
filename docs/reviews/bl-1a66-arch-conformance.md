# Arch & spec-conformance review — `docs/architecture.md` §0–§17 vs impl

**Ball:** bl-1a66 (parent epic bl-72a8) · **Reviewer:** Ciphered · **Date:** 2026-06-08
**Method:** every section's stated invariant checked against the source tree; all
load-bearing findings re-verified by hand against the cited `file:line` (not
trusted from a search summary). Spec quotes below are verbatim from the FROZEN
`docs/architecture.md`.

## Verdict

The core single-source-of-truth model is faithfully built — **id-is-path, no
stored status, the derived 3-state ladder, the generic blocker primitive,
`delivered_in` as a query, lazy `materialize`/fixpoint, the vanished
`resolve_remote`, the atomic seal, reverse-order rollback, the unified log** all
hold and are well-tested (see "Confirmed conformant" at the end). The gaps are
concentrated in two places: a **resolved §15 topic that regressed in code**
(the recursion cap), and the **consumer-facing read surface for derived/preserved
values** (`--json` bedrock + plugin stdout), which together strand the worktree
path the design goes to length to stage.

| # | Sev | § | One-line | Locus | Status |
|---|-----|---|----------|-------|--------|
| C1 | **critical** | §6 / §15 bl-7110 | Recursion cap *suppresses* plugins instead of *abort+rollback* — the exact disposition §15 retired | `src/plugin.rs:140-143,231-243` | ✅ FIXED (bl-abe5) |
| M1 | **major** | §3 / §9 / §11 / §15 bl-934a,bl-d074 | `--json` bedrock drops `task.extra`; unknown preserved keys are invisible | `src/reads.rs:218-235` | ✅ FIXED upstream (bl-e582) |
| M2 | **major** | §6 / §11 / §15 bl-934a | Worktree-path consumer surface entirely missing (no stdout print + core nulls plugin stdout) | `src/bin/bl-delivery.rs:20-25`, `src/plugin.rs:203` | ✅ FIXED (bl-abe5) |
| M3 | **major** | §8 / §14 | Landing-sealing family (`install`) bypasses the engine seal/rollback spine | `src/lib.rs:183-186`, `src/adopt.rs:74-76,121-127` | ✅ FIXED (bl-f387: `install::run` + `seal_copy` seal via the Engine; `prime --install` converges on the same spine — only its pre-materialize tracker fetch stays outside the §14 trace) |
| M4 | **major** | §4 / §6 / §15 bl-8540 | `[hooks]` schedule does not layer like other config (no XDG overlay, no `_prepend/_append/_ban`) | `src/hooks.rs:55-68` | ✅ FIXED (bl-abe5) |
| m1 | minor | §7 | Post-wire `current_state` (sealed after-state) never populated | `src/mutate.rs:71` | ✅ FIXED (bl-667e, strike) |
| m2 | minor | §2 / §6 / §15 | Stale doc-comments asserting retired mechanisms as current | `src/plugin.rs:140,22-25`; `src/op.rs:19,23`; `src/lifecycle.rs:17,152,218`; `src/layout.rs:124-136` | ✅ FIXED (bl-abe5) |

> **Resolution (bl-abe5 + bl-667e, 2026-06-08).** Six of seven fixed: **C1** (cap
> now aborts + emits an `error` record; the engine unwinds), **M2** (delivery
> prints the path on `claim.post`; core forwards plugin stdout via
> `Stdio::inherit`), **M4** (`Hooks::effective` layers landing ⊕ XDG `plugins.toml`
> through the shared `config::layer_over`, so `_prepend/_append/_ban` compose),
> **m2** (stale doc-comments corrected), and **m1** (STRUCK from §7 by subtraction
> — a `post` reactor derives the landed ball from git, not the wire; §14
> derive-don't-store; §15 entry + `OpContext.after` removed). **M1** was
> independently fixed upstream by bl-e582 (`task_json` now projects `task.extra`).
> **M3** is tracked by **bl-f387** — wiring the standalone `bl install` verb
> through the Engine seal (and converging `prime --install` onto it) resolves the
> install seal/rollback spine; the Author-first ambiguity dissolves because
> standalone install's `--from` is already synced (no fetch-before-copy), while
> `prime --install` stays the fetch-then-install composition. (The earlier bl-75f9
> was dropped as a duplicate of bl-f387.)

---

## C1 — Recursion cap runs the op PLUGIN-FREE instead of aborting (critical)

**§6 (frozen), and §15 resolution bl-7110, are unambiguous:**
> "**Crossing the cap ABORTS the op — fail, not silent:** core rolls the run
> plugins back in reverse order (§8/§14) and emits a diagnostic naming the
> op/plugin chain that overran… (The retired disposition — "run PLUGIN-FREE at the
> cap, suppressed not refused" — was the worst option: it converted a runaway into
> quiet wrong-behavior…) **There is no hatch to re-enable plugins on a nested
> call**".

The code implements precisely the retired disposition:

```rust
// src/plugin.rs:140-143
/// At the cap, the whole op runs plugin-free (§6) — `run`/`rollback` no-op.
fn suppressed(&self) -> bool { self.depth >= DEPTH_CAP }

// src/plugin.rs:231-243
fn run(&self, …) -> io::Result<()> {
    if self.suppressed() { return Ok(()); }     // op silently runs plugin-free
    self.invoke(…)
}
fn rollback(&self, …) { if self.suppressed() { return; } … }
```

At the cap, `run` returns `Ok(())`: the op **completes successfully without its
plugins**, emits no diagnostic, and names no overrun chain — the quiet
wrong-behavior §15 chose to forbid. The module doc (`src/plugin.rs:22-25`) and
the `suppressed()` doc (`:140`) even restate the retired wording verbatim
("suppressed, not refused… A plugin that wants nested plugins re-enables them on
its own nested call"), directly contradicting "no hatch to re-enable."

**Fix direction:** at `depth >= DEPTH_CAP`, return an `Err` that names the
op/plugin chain so the engine's existing reverse-order unwind (`lifecycle.rs`)
fires; delete the `suppressed()` short-circuit and the stale doc.

---

## M1 — `--json` bedrock silently drops `task.extra` (major)

**§3 (frozen):** `--json` is "the lossless mirror of stored frontmatter ONLY…
round-trip-safe" and "Unknown keys preserved on writeback (this is the opt-in
seam for a team's own `state:`/pipeline field)" — "read by their own display
plugin." **§11 (frozen)** makes a *stored* preserved key the worktree path's
authoritative read: the plugin writes `delivery-worktree` at `claim.pre` "so the
seal captures it and `bl show --json` surfaces it deterministically… **This is the
consumer's ONLY reliable read**."

`task_json` hand-builds the record over a fixed field list and never serializes
`task.extra`:

```rust
// src/reads.rs:218-235  (doc claims "lossless mirror of stored frontmatter ONLY")
json!({ "id": id, "title": …, "claimant": …, "priority": …, "parent": …,
        "tags": …, "blockers": blockers, "created": …, "updated": … })
// task.extra (#[serde(flatten)] toml::Table, src/task.rs:42-45) is NOT included.
```

`task.extra` round-trips correctly **on disk** (verified: `task_tests.rs` proves a
`state = "doing"` key survives parse→serialize), but it is invisible in `--json`.
Two spec promises break at once:
1. The §3 opt-in `state:` seam: a team's display plugin reading bedrock `--json`
   cannot see the key the seam exists to carry.
2. The §11 worktree-path machine read: `delivery-worktree` *is* staged into
   `extra` (M2 confirms the staging works), but `bl show --json` never emits it,
   so the "consumer's ONLY reliable read" returns nothing.

Note also a mechanism drift: §3 names the export as "`toml::Value → serde_json`"
(line 217); the code is a hand-listed `json!`, which is exactly why `extra` is
dropped. **Fix direction:** serialize the full `Task` (the `toml::Value → serde_json`
path the spec names) so `extra` flows through; keep the derived columns on the
human render only.

---

## M2 — Worktree-path consumer surface is unimplemented end-to-end (major)

**§11 (frozen)** surfaces the path two ways: "a HUMAN hint — on `claim.post` it
PRINTS the path on its stdout (§6), which balls forwards verbatim" and the
machine read via `--json` (M1). **§6 (frozen):** "stdout: the plugin's USER-FACING
channel — balls forwards it to the invoker's stdout **verbatim**."

Neither half works:
- The delivery binary's only `println!` is the `protocol` self-describe; nothing
  prints on `claim.post`:
  ```rust
  // src/bin/bl-delivery.rs:22-25
  if args.first().map(String::as_str) == Some("protocol") {
      println!("{}", delivery::PROTOCOL_JSON); return;
  }
  // claim.post → delivery::dispatch(...) materializes but prints no path
  ```
- Core does not forward plugin stdout — it discards it:
  ```rust
  // src/plugin.rs:203
  .stdout(Stdio::null())     // §7 "no return channel" — but §6 says FORWARD verbatim
  ```

So even if the plugin printed, core would swallow it. Combined with M1, the path
staged at `claim.pre` (works — `src/delivery.rs` stages `delivery-worktree`) is
**unreachable through any sanctioned channel**. The `bl skill` guide already
papers over this drift ("`bl claim` does not print the worktree path — find it
with `git worktree list` … `bl show --json` emits only canonical fields, so it is
not surfaced there"), i.e. the docs were written to the regressed code, not §11.

**Tension to resolve, not just patch:** §7 says "stdout… PARSES NOTHING back into
state (no return channel)" while §6 says balls "forwards it to the invoker's
stdout verbatim." These are compatible (forward to the human, parse nothing), but
the code collapsed both to `Stdio::null()`. The fix is to *forward* (inherit/tee)
plugin stdout to bl's stdout without parsing it, and have the delivery plugin
`println!` the path on `claim.post`. M1 fixes the machine read independently.

---

## M3 — `install` bypasses the §8 seal / §14 rollback spine (major)

**§8 (frozen):** "`config` / `install` keep the sealing shape but seal to the
LANDING, not the store… Symmetric in skeleton, different in target and mechanics."
**§14** then governs every sealing op with reverse-order rollback + core un-seal.

In code the sealing shape is absent for this family:
- Standalone `bl install` is **unwired** — it falls to the catch-all stub:
  ```rust
  // src/lib.rs:183-186
  other => { println!("{}", Op::new(other).plan()); Ok(()) }   // prints "install: pre -> ..."
  ```
  The doc at `src/lib.rs:150` concedes it: "diffless verb (`install`) is unwired,
  so it reports its §8 op plan." (`Verb::Install` is classed diffless in
  `src/verb.rs`, not a sealing op.)
- The only real adoption path, `prime --install`, runs `install.pre` through a
  **hand-rolled loop** (no engine, so no reverse-order rollback if a later step
  fails) and commits the landing with a bare `add -A && commit` — no change
  worktree, no atomic commit+integrate seal, no §14 un-seal:
  ```rust
  // src/adopt.rs:74-76
  for plugin in &pre { plugins.run(plugin, Verb::Install, Phase::Pre, landing, None)?; }
  // src/adopt.rs:121-127
  fn commit_landing(landing: &Path) -> io::Result<()> {
      git::run(landing, &["add", "-A"], None)?;
      if git::run(landing, &["diff", "--cached", "--quiet"], None).is_err() {
          git::run(landing, &["commit", "-q", "-m", "balls: install"], None)?;
      } Ok(()) }
  ```

**Mitigation (why major, not critical):** the adoption is local-only and
git-recoverable, and the tracker's `install.pre` is an idempotent fetch whose
rollback would no-op anyway, so practical blast radius is low. But the structural
invariant — "the sealing shape, sealed to the LANDING, under §14 unwind" — is not
realized, and a publish direction / arbitrary `--from/--to` install (§6) is not
dispatchable at all.

---

## M4 — `[hooks]` schedule does not layer like the rest of config (major)

**§4 (frozen):** "The `[hooks]` lists in `config/plugins.toml` (§6) ARE list
fields, layered the same way — a landing's schedule composes with an XDG
`_prepend` / `_append` / `_ban`". **§6** repeats it: "It is layered + merged
exactly like the rest of config (§4) — a center's schedule composes with an XDG
`_prepend`/`_append`/`_ban`… so there is NO parallel … mechanism."

`Hooks::load` reads a single landing file and parses only `[hooks]` — it never
goes through `EffectiveConfig::resolve` / `layer_over` / `compose_list`, and there
is no XDG `plugins.toml` overlay:

```rust
// src/hooks.rs:67-68
pub fn load(landing: &Path) -> io::Result<Hooks> {
    Hooks::load_from(&landing.join("config").join("plugins.toml"))   // landing only
}
// parse() (:38-53) reads [hooks] verbatim — no _prepend/_append/_ban, no XDG merge
```

`compose_list` exists and is well-tested, but only `balls.toml` uses it; the hook
schedule is read raw. The spec's "ONE layering mechanism, not two" is half-built:
`balls.toml` layers, `plugins.toml` does not. **Fix direction:** route the
`[hooks]` table through the same XDG-over-landing + `compose_list` path as other
list fields.

---

## m1 — Post wire `current_state` is always null (minor)

**§7 (frozen) post payload:** "`previous_state` / `current_state` (`null` for
create / close-drop respectively)" — i.e. for `claim`/`update`, `current_state`
should be the sealed after-state `Task`. `OpContext.after` is hardcoded `None`:

```rust
// src/mutate.rs:66-72
let ctx = OpContext { actor, binding, command: Some(command(verb, &flags)),
                      before, after: None };
```

So a `post` reactor never receives the landed `Task` on the wire; it must derive
it from `metadata` (the `bl-id` trailer) + git. This is consistent with §14's
"post never mutates; derive-don't-store," and no shipped plugin needs it, so
impact is low — but the §7 field is effectively unimplemented. Either populate it
from the sealed tree or strike it from §7.

## m2 — Stale doc-comments assert retired mechanisms as current (minor)

The task asks to flag "drift between §15 resolved topics and the code." Several
doc-comments describe mechanisms §15 explicitly retired, as though current:
- `src/plugin.rs:140` & `:22-25` — restate the **retired** "suppress, not refused
  / re-enable on a nested call" cap disposition (bl-7110). *Tied to C1 — fix together.*
- `src/op.rs:19,23` and `src/lifecycle.rs:17,152,218` — reference the **retired**
  `NN-`-prefixed filesystem plugin registry (bl-8540 moved the schedule into
  `plugins.toml`). The code correctly reads `Hooks`; the comments lie.
- `src/layout.rs:124-136` — describe a "store coincides with the landing / doubles
  as the store… resolves to `Self::landing`" path that §2 says must **not** be a
  code path ("NO branching on whether those refs are equal"); the code correctly
  has no such branch, but the comment implies one.

Comments are not behavior, but in a project whose §15 discipline is "frozen means
no silent edits," doc-comments asserting retired designs are exactly the drift the
audit is meant to surface.

---

## Confirmed conformant (re-verified, high confidence)

These invariants HOLD in code, with tests:
- **§3 id-is-path** — `Task` has no `id` field; id derived from filename basename
  (`src/task.rs:21-49`, `src/taskfile.rs:17-65`). No index.
- **§3 no status/state field; derived 3-state ladder; "blocked" = claim-blockers
  only** — `Task::status` short-circuit (`src/task.rs:120-132`); exactly three
  live states.
- **§3 priority absent-sorts-last** — `order_key` leads with `priority.is_none()`
  (`src/reads/list.rs:74-77`).
- **§10 the blocker is generic over `on`** (the load-bearing test): `on = Verb`,
  not a claim/close enum; `enforce::gate` rejects any op whose blocker names it
  (`src/enforce.rs:52-58`); `--parent` mints no edge; `--needs`/`--no-needs`
  editable via `update`, `--blocks` create-only (`src/mutate_build.rs`).
- **§11 `delivered_in` is a derived `git log --grep` query, recency-ordered, no
  field** (`src/delivery_repo.rs:93`).
- **§2/§12 lazy `materialize` (orphan iff ref absent), core-owned bounded
  fixpoint, bl-fa00 reset reverted** (`src/substrate.rs:65-82`,
  `src/lifecycle_diffless.rs:83-115`, `src/checkout.rs:90-111`).
- **§15 bl-a476/bl-e060 core `resolve_remote` GONE** — zero hits in core; origin
  discovery is the tracker's, once, from the invocation path
  (`src/tracker/mod.rs:100-135`).
- **§12 bl-9857 push-failure split** (founding-miss → silent stealth; established
  reject → E5 error) — distinct paths (`src/tracker/prime.rs`,
  `src/tracker/remote_ops.rs`).
- **§8 seal = commit + ff-integrate, anvil-atomic; §14 reverse-order rollback,
  pre-discard vs post-reset, pre-only plugin rolled back on post failure,
  best-effort exit-ignored** (`src/git.rs:95-106`, `src/lifecycle.rs:189-247`).
- **§8 close-blocker check is abort-safe (before delivery & seal)**
  (`src/change.rs:262-270`).
- **§1 unified per-clone log; bl-e6a0 line bound (≤ PIPE_BUF, char-boundary
  truncation, re-serialize-to-measure, lock-free O_APPEND); bl-2e9f dead
  `layout::log()` deleted** (`src/log.rs:35-169`).
- **§5 trailers via `git interpret-trailers` (no hand-rolled parser); subject =
  title; `bl-id` present on per-task ops, absent on checkout-scoped; no
  `bl-from-state`/`bl-to-state`** (`src/message.rs`).
- **§6 install path-copy semantics** (folder=mirror, glob/file=union, siblings
  untouched, `bin/` excluded, `N added/M deleted` summary, protocol-validated
  bind) — the *library* (`src/install.rs`) is faithful; only the *dispatch* is the
  M3 gap.
