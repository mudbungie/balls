# bl-3616 — store tiers: publication is a verb, boundaries are plugins, pointers are tags

**CONVERGED (2026-09-17, maintainer dialogue; bl-d40a).** Every §6 question closed by dialogue, Q7's position ratified (*"Variables are a smell, but not banned. They do exist for reasons."*). Implementation minted in §8. Originally **PROPOSED (2026-09-04, Inflate).** Filed from the maintainer's reframe of how
balls meets a team: *balls should lifecycle at high velocity locally and under
the user's name; shared stores exist but get deliberately mirrored work; the
next tier up is the corporate tracker (jira / GitHub issues — tasks for humans,
aligned to a roadmap goal); a ball's counterpart one tier up is sometimes a
direct mirror, sometimes the whole of which it is a component.* Wanted: (a)
bidirectional pointers to balls in other stores / repos and to external
trackers, (b) a sync-policy ladder — mandatory replication (today), warnings at
various obnoxiousness levels, explicit opt-in sync. This document states the
maximally-subtracted design first and asks the maintainer to argue it up. §6 is
the part that is NOT settled.

**Amended 2026-09-17 (bl-5af5): Q3/Q4/Q5 closed by the maintainer; Q7 (bl-1266 H1 now reachable) opened. All of §6 is closed except Q7, which is an implementation prerequisite, not a design question. Ready to mint §8.**

**Amended (2026-09-06, Inflate, bl-22c5).** The maintainer attacked §2 claim 1:
*"locally I will have a bunch of agents running with subagents, so balls is a
multi-writer blast radius, even at the local scale. It's never single-writer."*
Conceded. The premise was wrong AND unnecessary: the invariant that makes
ff-only sufficient today is not "one writer" but "a rejected push un-seals, so
the local store is never ahead of the remote by more than the in-flight op."
§2 claim 1 and §3 are rewritten around the mechanism that replaces that
invariant — a rebase of local seals — and §6 Q1/Q2 are closed by it. Everything
else stands.

## 1. What exists, exactly (verified against `main` 93a0ab7f)

- **One store per checkout**, a branch (`tasks_branch`, default `balls/tasks`).
  The remote resolves per op by the §12 ladder (`--remote` > stealth sentinel >
  `task-remote` binding > `origin`).
- **Publication is a side effect of every mutation.** The `[hooks]` seed wires
  `bl-tracker` on `create.post`, `update.post`, `claim.post`, `unclaim.post`,
  `close.post` (plus `sync.pre` / `prime.pre` / `install.pre` for the fetch).
  `remote_ops::push` runs `git push <remote> <tasks_branch>`; a rejection on an
  established store is **E5** — *"the mutation did not land; the op aborts"*
  (§12). Currency is optimistic: the push IS the contention check.
- **"Unsynced" is not a thing the store knows.** There is no field, no marker.
  It is nevertheless fully computable: `git rev-list --count
  <remote>/<branch>..<branch>` (ahead) and the reverse (behind).
- **No pointer field.** §3 (bl-3067) REJECTS pure-metadata link types and names
  `tags` as the home of relatedness: *"relatedness is an equivalence CLASS, not
  a pairwise edge, and `tags` names the class: `list --tag` returns the whole
  cluster in one query."* `Task::extra` (free TOML keys, set by `key=value` on
  create/update) is the other opt-in seam, but `list` cannot filter on it.
- **`parent`** is a single scalar, *"containment only: builds the display tree,
  gates nothing"*, with no liveness check on the id it names.
- **A tier-2 plugin already exists and already made the fail-open call.**
  balls-github-plugin's `github-issues` mirrors create/update/close to GitHub
  Issues, pulls external closes down on `sync`, and since bl-a95c *"fails OPEN on
  transport errors instead of aborting the local op."*
- **Centers** (§12) are the existing shared-store shape: `bl prime --center
  <hub>` binds a checkout's store to a hub; `list --everywhere` reads the whole
  fleet. A center is ONE store with many writers — which is exactly where E5
  bites.

## 2. The reframe that dissolves the request

The complaint is "fail-closed sync." The cause is not the fail-closed policy;
it is **one store branch with many writers, published as a side effect of every
op.** Split those two and the ladder falls out:

1. **Every store is multi-writer; the question is only WHEN seals publish and
   HOW divergence reconciles.** (Rewritten under bl-22c5.) On one box, N agents
   seal into one local branch — already serialized by the §0 CAS commit point,
   no remote involved. Across boxes (and cloud sessions) they publish to one
   remote, so the remote branch has many publishers. Today that is safe only
   because a rejected push un-seals: the local branch is never more than one
   op ahead, so ff-only import always works and E5 is the retry signal. That
   invariant is the real cost of fail-closed, and deferring publication breaks
   it on purpose. What replaces it: **reconcile = fetch, rebase local seals
   onto the remote tip, push.** Each seal is one commit touching one
   `tasks/<id>.md`, so seals on different balls rebase clean by construction;
   only a same-ball race can conflict. Transport failure is then a non-event
   (the store is *ahead*, drift renders it), and E5 narrows to "this ball was
   changed on both sides."
2. **A shared store is a different store, reached across a boundary.** Work
   arrives there by a deliberate act (check-in), per ball, not by branch push.
3. **Every tier boundary has the same shape**: a counterpart one tier up, a
   plugin that knows how to read and write it, four hooks (`create/update/close
   .post` to push, `sync` to pull, `show`/`list` to render the drift). The
   github-issues plugin IS this shape today. A "shared bl store" boundary is the
   same plugin contract with git as the transport.

Under that reading, nothing new is stored and nothing new is a mode. Each item
the maintainer asked for maps to an existing explicit signal:

| Ask | Home | Mechanism |
|---|---|---|
| mandatory replication (today) | schedule | plugin wired on `*.post` |
| explicit opt-in sync | schedule | plugin wired on `sync.pre` only (delete the `*.post` lines) |
| warn every action | render | drift line on `bl list` header (every session reads it) |
| warn only when you cause the desync | render | the same plugin's `*.post` prints the ahead count to stderr instead of pushing |
| does the mismatch live on the ball or the remote? | neither | it is the diff of two refs (tier 0→1) or two `updated` stamps (tier 1→2), computed at read |
| closed balls: re-queried? | plugin policy | `close.post` acts on the counterpart (mirror: close it; component: annotate it); `sync` pulls the counterpart's fate down |
| pointer to a ball in another store / repo | tag | `up:<store>#<id>` |
| canonical pointer to an external tracker | tag | `jira:PROJ-123`, `gh:owner/repo#42` — the plugin owns its namespace |

The severability test holds: turning "mandatory" into "opt-in" deletes two
schedule lines; turning it back re-adds them. No code edit, no config value.

## 3. Publication is a verb: `bl sync` grows a push

Today `bl sync` is fetch + fast-forward only. Under §2 it becomes the ONE place
the personal store publishes when the tracker is not wired on `*.post`:
fetch-ff, then push. This is not a new verb — `prime.post` already does exactly
*"settle store content (fetch-ff + push)"* through the tracker. `sync` and
`prime` are the same act at two moments.

**One mechanism, two moments** (rewritten under bl-22c5). The tracker gains a
single reconcile: `fetch`, then `rebase <remote>/<branch>` of every local seal
not yet on the remote, then `push`. It is called from two places:

- **Per-op `*.post`** (mandatory wiring): push; on a non-ff reject, reconcile
  ONCE — there is exactly one local seal, the in-flight op's — and push again.
  A clean rebase means the contention was on a *different* ball, which is the
  common case with many agents, and the op lands with no human in the loop.
  A rebase conflict means the SAME ball changed on both sides: abort the
  rebase, un-seal, and E5 stands with a sharper sentence that names the ball.
  §12's "deliberately NO pre-pull" argument survives intact — this is a
  post-reject pull on the contended path only, no round-trip on the happy path.
- **`bl sync`** (opt-in wiring, and prime): the same reconcile over N seals.
  A conflict aborts the rebase, leaves every local seal local, and names the
  ball; the drift line keeps showing `ahead` until the operator resolves it in
  the store checkout — the recovery `bl sync --skill` already prescribes for
  the crash-between-seal-and-push shape, now the ordinary conflict path too.

Same-ball conflicts are semantically meaningful, and refusing is the right
verdict in each: two claims of one ball (both add `claimant`) — the later
loses, which IS claim contention; a close against a remote update
(delete/modify) — a human decides whether the update mattered. No field-wise
merge, no CRDT (§7).

**Transport failure fails open** in both moments (warn on stderr, store stays
ahead) — the line github-issues drew in bl-a95c, moved into the tracker. Only a
same-ball conflict or a permissions reject remains fail-closed.

**Occupancy may stay eager while content defers.** Two boxes claiming the same
ball unknowingly is the one race deferral makes worse. The wiring is per hook,
so `claim.post`/`unclaim.post` can keep the tracker while `create/update/close
.post` drop it: claims publish now, everything else at `bl sync`. That is the
"obnoxiousness level" the maintainer asked for, expressed as which hooks carry
the plugin — still no mode, still no config value.

**Drift render.** bl-tracker gains a read-op hook (`list`, `show` — the bl-0af4
single-phase dispatch bl-delivery already uses for the `worktree` line) that
folds one line into the human render: `store: 3 ahead, 0 behind
origin/balls/tasks`. Derived from `git rev-list`, never in `--json`, absent in
stealth. This is the whole "obnoxiousness" surface for tier 0→1.

## 4. Pointers are tags, stored once, pointing UP

The maintainer's own framing: *"it's up to a jira plugin to know how to find the
local tasks."* That is the single-source-of-truth answer. The pointer lives on
the LOWER ball only, as a tag in the plugin's namespace. The upper side never
stores a back-pointer; it queries: `bl list --tag jira:PROJ-123 --all` returns
every local ball — live or dead — that is a component of PROJ-123. Bidirectional
means navigable both ways, not stored in both places.

Why a tag and not `parent` or an extra:

- `parent` is one scalar. A ball that is a subtask of a local epic AND checked
  into a shared store has two "ups" at once. Chains fit `parent`; a ball that
  faces two tiers does not.
- extras are not queryable by `list`, so the upper tier could not find its
  components without reading every file. Tags are the class query §3 already
  argued for.
- A tag renders in the `list` row. The maintainer asked for sync state to be
  visible; the pointer being visible on every row is the cheapest form of that.

**Mirror vs component collapses.** A mirror is a component whose upper
counterpart has exactly one component; the only behavioral difference is who
closes the upper. That is plugin policy at `close.post` (github-issues closes the
issue; a jira plugin may instead comment "component bl-3616 closed"), not a
kind of pointer. One tag shape, no `kind=` attribute.

**Cross-store pointer spelling.** `up:<store>#<id>`, where `<store>` is a git
URL or a branch of the current repo (`up:balls/team#bl-12ab`,
`up:git@host:hub.git#bl-12ab`). Ids are 4-hex and collide across repos, so the
store qualifier is load-bearing; within one store the id alone is the pointer.
Implementation check: tag validation charset must admit `:`, `/`, `@`, `#`.

## 5. The shared bl store as a plugin (`bl-upstream`, not built) — rewritten under Q3

The one genuinely new thing is a plugin that treats another bl store the way
github-issues treats GitHub — with the difference that the other store is a
**founded checkout** it operates only through `bl -C`. Same four hooks:

- **founding** (once, at `install`/`prime`): `bl -C <territory>/<remote>/
  prime --remote <shared>` — the shared store lives in the plugin's §1
  territory like any other keyed checkout. Nothing is invented: it is a
  checkout keyed by a path.
- `create/update/close .post` — for a ball carrying an `up:` tag, mirror the
  op through the shared store's own verbs: `bl show <id> --json | bl -C <dir>
  import` for content, `bl -C <dir> close <counterpart>` when the plugin's
  policy says a close propagates (mirror) — every seal CAS, seen-token and hook
  the shared store has wired applies, because it IS a store.
- `sync` — `bl -C <dir> sync`, then for each `up:`-tagged ball whose
  counterpart is newer, `bl -C <dir> show <id> --json | bl import` (or refuse
  with the diff — the same seen-token discipline close uses).
- `show`/`list` — the drift render of Q5, from the two `updated` stamps.

Wired on `*.post` it is mandatory replication; wired on `sync.pre` alone it is
check-in on demand. The ladder is the schedule, again. The cost this shape
carries is Q7: a nested `bl -C` on a different store must publish, and today
it does not.

## 6. Ledger — every question, how it closed (was: not settled)

1. ~~Is the personal tier really single-writer?~~ **CLOSED by the maintainer
   (2026-09-06): it is never single-writer.** Resolved by §2.1/§3 — the
   rebase reconcile does not need the premise.
2. ~~Does deferred publication need a merge, not an ff?~~ **CLOSED with (1):**
   yes — a rebase of local seals, refusing on same-ball conflict and naming
   the ball. Residue (sha pinning) audited and CLOSED in §6.1.
3. ~~Addressing a second store of the same project.~~ **CLOSED by the
   maintainer (2026-09-17), against my position:** *"clone it in. We already
   have the mechanisms to have local checkouts safely stored alongside each
   other, and this keeps all operations along the same path, reduces the code
   forks. Doing plumbing like that is exactly how we introduce bugs by shimming
   in under the existence of other controls."* So the shared store is a
   **founded checkout**, not a branch the plugin writes by plumbing: the plugin
   founds it once in its own territory (`bl -C $XDG_STATE_HOME/balls/plugins/
   bl-upstream/<remote>/ prime --remote <shared>` — the §1 keyed-by-path layout
   IS the "mechanism to store checkouts alongside each other"), and every
   operation on it is a `bl -C <that-dir> <verb>`: check-in is `bl show <id>
   --json | bl -C <dir> import`, check-out is the same pipe reversed, sync is
   `bl -C <dir> sync`. Seal CAS, seen-tokens, chore minting, the tracker's own
   reconcile — all of it applies to the shared store because it is a store,
   not a branch the plugin pretends is one. §5 is rewritten below. **This
   reaches bl-1266's H1** — see Q7.
4. ~~Tag namespace as protocol.~~ **CLOSED (2026-09-17, maintainer: "I agree,
   that's fine").** A plugin's tag prefix is its name; no registry. Residue,
   accepted: a hand-typed `jira:whatever` with no plugin is free text until a
   plugin reads it.
5. ~~Does the drift line belong on `list` or on `conf`?~~ **CLOSED by the
   maintainer (2026-09-17):** *"drift should be detected, though not
   necessarily reconciled, at every op, in the scope of the op. A list is
   gonna get everything, but drift at the ball level would be shown at bl
   show."* So three surfaces, one derivation, nothing stored:
   - **every mutating op**, on stderr (the confirmation channel), for the op's
     own ball: `bl-3616: 2 seals unpublished` — the tracker's `*.post` computes
     it whether or not it is wired to push (with mandatory wiring it reads 0
     after the push).
   - **`bl show <id>`**: a per-ball line, `published: behind by 2 seals (last
     fetch 3h ago)` for tier 0→1 (`git log <remote>/<branch>..<branch> --
     tasks/<id>.md`), and the plugin's `updated`-stamp comparison for tier 1→2
     (`up: balls/team#bl-12ab, counterpart newer by 2h`).
   - **`bl list`**: the store-level header (ahead/behind counts) — list gets
     everything, so it gets the aggregate, not a column.
   **The one caveat, held:** "detected at every op" is free for *ahead*
   (local, no network) and only as fresh as the last fetch for *behind*. §12's
   refusal of a per-op pre-pull stands (*"it would add a remote round-trip to
   every op"*), so behind-drift is reported against the tracking ref and
   stamped with the fetch age, never fetched per op. Detection is honest about
   its own staleness rather than paying a round-trip to hide it.
6. ~~Should occupancy be eager by default in opt-in wiring?~~ **CLOSED by the
   maintainer (2026-09-17): it is configurable.** *"Users will often want to
   check things out aggressively, but not always. They may also want to use
   identity shims to indicate agent/user relationship. It's an injection point
   for plugin configuration."* So balls picks no default: which hooks carry the
   tracker is the per-checkout `[hooks]` schedule (already plugin configuration,
   §6 of the architecture), and the seed documents the two shapes (§3) without
   choosing. **Identity shims** ride the same seam: `--as ID` is the one identity
   injection point (every seal carries a `bl-actor` trailer), and a plugin that
   publishes upward may rewrite or qualify it — `mark/Inflate` on the shared
   store, `Inflate` locally — as its own config, not a core field. The
   agent→user relation is therefore a plugin's rendering of the actor trailer,
   never stored on the ball (§0: don't store what you can compute).

7. **Q3 makes bl-1266's H1 real — nested-op non-publication must become
   store-scoped.** bl-1266's rule is *"an op publishes only if it is the
   outermost `bl` in its invocation tree"*, enforced by `BALLS_PLUGIN_DEPTH`
   (`Env::nested()` = depth ≥ 2 → `push` no-ops). Its own record already names
   the hole: *"A plugin that shells `bl -C otherrepo create` publishes to a
   DIFFERENT store with a different remote. Depth suppresses that too, leaving
   the far store sealed-but-unpublished … a debt nobody pays … a real hole, not
   a rounding error — it is just an unreachable one today."* A `bl-upstream`
   that shells `bl -C <shared> import` is exactly that plugin, so H1 is now
   reachable. bl-1266 wrote the fill: *"core exports the store path it holds
   into every plugin spawn (inherited through the shelling plugin, exactly as
   depth is), and the tracker suppresses iff `binding.store` matches it —
   failing OPEN when unset."* Its stated cost: one new env, which §6 of the
   architecture calls a smell. Position: pay it — the true rule (*"an op does
   not publish an anvil an enclosing op holds open"*) is store-scoped, depth
   was only ever a proxy that happened to coincide while every nested op was
   in-store, and the maintainer's Q3 answer chose "same path, no plumbing" at
   the price of nesting. The alternative — the plugin scrubbing the depth env
   before shelling — is precisely the "shimming in under the existence of
   other controls" Q3 rejected. Implementation belongs to bl-1266's residue,
   not here; this doc only records that the condition it was waiting on has
   arrived.

## 6.1 Audit — what pins a sha, what pins a ref (bl-eb3e, 2026-09-17)

The bl-22c5 reconcile rebases local, unpublished store seals: content, trailers,
timestamps and order survive; commit shas do not. The maintainer asked for a
clear line on what should pin a hash versus a ref. **The rule:**

> A sha may be pinned only if it is **content-addressed** (a blob or tree —
> stable across any rebase) or **published** (on the remote store branch, which
> only ever advances by fast-forward and is never rewritten). A commit sha of an
> **unpublished local seal** is scratch: it may be read within the op that
> observes it, never persisted. Everything else is a **ref** (a branch name, a
> tag, a path), re-resolved on every read.

Every pinning site on `main` (93a0ab7f), checked against that rule:

| Site | Pins today | Kind | Under rebase |
|---|---|---|---|
| seen-token (`src/seen.rs`, bl-9f1d) | `balls-seen/<id>` = the task file's **blob** sha; the "closer's last touch" anchor is found by walking `git log -- tasks/<id>.md` for a `bl-actor` trailer, fresh per call | content + transient | **safe** — blob unchanged, trailer walk re-resolves |
| verdict cache (`src/speculate.rs`, bl-1263) | `(tree OID, gate fingerprint)` of the CODE worktree | content | **unaffected** — code repo, not the store |
| merge queue `merging/<id>` (`src/speculate_queue.rs`) | the `work/<id>` CODE tip commit | published-side code sha | **unaffected** — code repo; work branches are not rebased by the reconcile |
| delivery base / moved-target refusal (`bl close`, bl-a1a4) | the target ref's pinned CODE sha, one op | transient | **unaffected** — code repo |
| `root_commit` (task field, bl-1ce7) | the project's root commit (`rev-list --max-parents=0`) | content-stable identity | **unaffected** — the code repo's root, immutable by definition |
| wire payload `commit` / `previous_commit` (`src/wire.rs` post facts, §7) | the seal just made, threaded to `*.post` plugins | transient (unpublished) | **rule applies** — a plugin may act on it within the op; none persists it today (tracker, delivery, chore, github-plugin, adversary all checked). The reconcile itself runs in `*.post`, so a plugin ordered AFTER the tracker would see a sha that no longer exists — the tracker already sorts LAST (§14), which is what makes this a theorem, not a hazard |
| journal (`src/reads/journal.rs`), closed-id resolution (`show` fallthrough), claim-age | store history walked by PATH and ORDER; renders timestamps and trailers, never a sha | ref | **safe** — rebase preserves order and content |
| op-log liveness (bl-8750) | the op log's tail timestamp | content | **safe** |
| §8 one-HEAD-per-op invariant (bl-057a) | *"the SEAL's `merge --ff-only` advances the CHECKOUT itself … nothing advances the store branch by plumbing behind it. Any future op that does advance `balls/tasks` without moving the checkout breaks this, and breaks it silently."* | ref (the checkout IS the branch) | **CONSTRAINT** — the reconcile must be a `git rebase` run IN the store checkout (the tracker's cwd), never `update-ref` plumbing. That is the same shape as today's `merge --ff-only` import, so no new mechanism, but the implementation ball must state it |
| a sibling op in flight during the reconcile | its change worktree forked from the old tip; its seal is a `merge --ff-only` onto the checkout | §0 CAS | **safe by design** — the ff loses (old tip is no longer an ancestor), the seal CAS fails, converge-on-retry re-authors against the new tip. The reconcile is just another writer to the local branch |
| remote store branch | the linearization point | ref, ff-only, never rewritten | **the invariant** — only unpublished seals are ever rebased; the nested-op rule (bl-1266, no push from a nested op) is what guarantees "local ahead of remote" is exactly the rebasable set |

**Findings.** Nothing on `main` pins an unpublished store-seal sha. Two things
the implementation must carry: (1) the reconcile rebases the checkout, not the
ref (§8 bl-057a); (2) the seen-token doc comment and §7's wire description
should state the rule above, so the next plugin author knows `commit` is
evidence for this op, not a handle to keep. The residue in §6 Q2 is CLOSED.

## 7. What this does NOT solve, stated

- Two writers editing one ball on a shared store still conflict; the plugin
  refuses with the diff. No CRDT, no field-wise merge.
- A ball checked in under a tag and then hand-deleted upstream is an `up:` tag
  pointing at a dead counterpart; `sync` reports it, nothing repairs it.
- Nothing here changes tier-2 plugins: github-issues already has the shape.
  A jira plugin is a port, not a design.

## 8. Implementation balls (minted 2026-09-17 on convergence)

- **bl-21ab** — bl-tracker reconcile: fetch + rebase local seals IN the store checkout + push; once-on-reject from `*.post`, over N seals from `sync`; transport fails open; pinning rule stated in `src/seen.rs` + §7. **BUILT 2026-09-18.**
- **bl-439d** — drift render: `*.post` stderr count for the op's ball, `show` per-ball line with fetch age, `list` header aggregate.
- **bl-331a** — tag charset admits `:` `/` `@` `#`; pointer convention documented. **BUILT 2026-09-18.**
- **bl-aac7** — bl-1266 H1 fill: store-scoped nested-op publication (core exports the held store path; tracker suppresses only on match, fail open when unset). **BUILT 2026-09-18.**
- **bl-260e** — seed `[hooks]` comment: the wiring shapes (mandatory / opt-in / occupancy-eager), no default; identity shims noted.
- **bl-5273** — `bl-upstream` plugin, sibling repo; a founded `bl -C` checkout, never plumbing. Needs bl-aac7 and bl-331a.
