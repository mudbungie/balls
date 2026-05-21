# SPEC: Federated Multi-Repo State

Status: draft
Scope: introduces an opt-in deployment in which several code repos share **one** `balls/tasks` hub, via a balls-owned state clone and a committed pointer file. The default single-repo orphan-branch model ([SPEC-orphan-branch-state.md](SPEC-orphan-branch-state.md)) is unchanged — a repo with no pointer behaves bit-identically to it.

This document is the authoritative contract for the federated model. README §"Multi-repo" and SKILL.md §"Multi-repo: one project, many repos" are the operational summaries; where they disagree with this file, this file wins.

## 1. Motivation

The single-repo model welds the task store to the code remote: `balls/tasks` is an orphan ref negotiated against the code's own `origin`. One repo, one disjoint task store. That is correct for a solo project and is not changing.

It does not fit a project that spans several code repositories:

1. **One project, many code repos.** A product with a `frontend`, an `api`, and an `infra` repo wants **one** backlog — one ready queue, one set of epics, cross-repo dependency edges. Under the single-repo model each repo carries its own `balls/tasks`; `frontend`'s `bl ready` can never see a ball filed from `api`. There is no shared upstream because the state ref has no remote of its own — it rides the code remote. "Several repos syncing to a single upstream" is structurally impossible without decoupling the two.

2. **External-tracker plugin sprawl.** A multi-repo project usually wants one external system (Jira, Linear, GitHub Issues) as the human-facing record. Installing the issue-tracker plugin in every code repo means N copies of the credential, N concurrent scheduled syncs racing each other, and N places where mirroring policy can drift.

The fix for both is the same: give the state branch a remote of its own. The **hub** is a dedicated git repo that carries only `balls/tasks` — no code, usually bare. Each code repo points at the hub instead of negotiating `balls/tasks` against its own `origin`. One backlog, one config, one place to run the bridge plugin. The code remote is never touched.

This is a real scope expansion over [SPEC-orphan-branch-state.md](SPEC-orphan-branch-state.md), taken deliberately. It does **not** abandon the project thesis: the hub is still git — no database, no daemon, no service — so "one VCS does both jobs" still holds. What changes is that for a participant repo the task store now lives in a *different* git repo than its code; §2 and §14 bound the consequences of that.

## 2. Principles

Invariants. An implementation that violates one is out of scope; revisit the spec instead.

1. **Default is unchanged.** A repo with no `.balls/master.json` pointer (and no legacy in-canonical pointer fields) behaves bit-identically to SPEC-orphan-branch-state.md. Its task store is the `.balls/worktree/` orphan worktree on the project's own git. Existing tests are the regression suite.
2. **Opt-in via a committed pointer, never convention or env.** Federation is a fact recorded in a tracked file — `.balls/master.json` — so a `git clone` carries it and every participant agrees. No environment variable, no per-invocation flag, switches the model.
3. **Still one VCS.** The hub is an ordinary git repo. No database, no daemon, no network service. Everything `bl` does against the hub is `git fetch` / `git merge` / `git push` against `balls/tasks`.
4. **The pointer is the only durable federation artifact.** `.balls/state-repo/`, the `.balls/config.json` / `.balls/plugins/` symlinks, and the `.balls/tasks` symlink are all gitignored runtime state, fully re-materializable from the pointer by `Store::discover`. Deleting them loses nothing durable — the README "local cache is disposable" principle, extended to the federation scaffold.
5. **Bilateral mobility.** A repo moves standalone→federated (`bl remaster <url> --commit`) and federated→standalone (`bl remaster --detach`) with no task-data loss. Detach must work **offline** — a repo must never be trapped in a federation whose hub it cannot reach.
6. **Hub wins, outright.** In federated mode the hub owns *all* policy — task knobs, `target_branch`, `delivery`, `review`, plugins. This is not a merge and not a runtime layering: the project-side `.balls/config.json` and `.balls/plugins/` *become symlinks into the hub clone*. There is no precedence chain to reason about because there is only one file.
7. **Hard-fail first-time, soft-fail warm.** First-time materialization against an unreachable hub aborts loudly and rolls back its scaffold. Once a warm cache exists, an unreachable hub is a soft-fail: `bl` works from the cache. Silently dropping a first-time client to a local orphan is forbidden — it is the exact drift the model exists to prevent.
8. **Hand-operable.** The federated layout is stock `git clone` plus `ln -s`. A human with `git`, `ln`, and `jq` can stand up a hub, join it, read state through the symlinks, and detach — §15. A change that breaks the hand sequence is a breaking change to this spec.

## 3. Terminology

- **hub** / **state hub**: a dedicated git repo whose `balls/tasks` ref is the shared task store. Usually bare and code-free; a real code repo may double as the hub if one participant is the natural source of truth.
- **pointer**: `.balls/master.json` — the committed file that records the hub link. Two fields, both optional.
- **`master_url`**: the hub's git URL. The recommended federation mechanism.
- **`state_remote`**: legacy — the *name* of a project-side git remote whose `balls/tasks` is negotiated against. Deprecated for the cross-repo case (§13).
- **state-repo**: `.balls/state-repo/` — a balls-owned git clone of the hub, separate from the project's own `.git/`. Where federated task state physically lives.
- **canonical config**: `.balls/config.json` — the config `bl` actually reads. A real file in a standalone repo; a symlink into the state-repo in federated mode.
- **federated flip**: `bl remaster <url> --commit` — the transition that turns a standalone repo into a federated participant.
- **standalone**: a repo with an empty/absent pointer — the SPEC-orphan-branch-state.md model.
- **bridge**: the single federated participant that runs an external-tracker plugin on behalf of the whole federation (§12; operational detail in SKILL.md).

## 4. Topology

A repo resolves to exactly one of two layouts at `Store` construction. The decision is the single seam `state_worktree_for()` (§8).

```
standalone (default — SPEC-orphan-branch-state.md, unchanged):

  project/.git/                      project's own git
  project/.balls/worktree/           orphan worktree on balls/tasks
  project/.balls/tasks  -> worktree/.balls/tasks
  project/.balls/config.json         real file, committed on the integration branch
  project/.balls/plugins/            real dir,  committed on the integration branch
                                     balls/tasks negotiated against the code `origin`

federated (opt-in — this spec):

  hub.git                            dedicated repo, carries only balls/tasks
        ▲           ▲           ▲
        │           │           │  each participant: a balls-owned clone
  ┌─────┴────┐ ┌────┴─────┐ ┌───┴──────┐
  │ repo A   │ │ repo B   │ │ repo C   │
  │ .git/    │ │ .git/    │ │ .git/    │   project git — code `origin` untouched
  │ .balls/  │ │ .balls/  │ │ .balls/  │
  │  master.json  ← committed pointer: { "master_url": "hub.git" }
  │  state-repo/  ← balls-owned clone of the hub (gitignored)
  │  tasks       -> state-repo/.balls/tasks
  │  config.json -> state-repo/.balls/config.json   (gitignored symlink)
  │  plugins     -> state-repo/.balls/plugins       (gitignored symlink)
  └──────────┘ └──────────┘ └──────────┘
```

In both layouts `claim`, `review`, `close`, `sync`, `create`, `list` are unchanged: only *which directory* the state-branch git operations run in differs, and that is resolved once and cached.

## 5. The `master.json` pointer

`.balls/master.json` is the committed bootstrap pointer (`src/master_pointer.rs`). It carries only the fields `bl` must read *before* the canonical config resolves — and in federated mode the canonical config *is a symlink into the state-repo*, so `master_url` cannot live there: bl could not find the state-repo to follow the symlink.

```json
{ "master_url": "git@host:proj-hub.git" }
```

| Field | Type | Default | Meaning |
|---|---|---|---|
| `master_url` | string \| null | null | Hub git URL. When set, `bl` materializes `.balls/state-repo/` and routes every state-branch op through it. |
| `state_remote` | string \| null | null | Legacy (§13). Name of a project-side git remote. `null` resolves to `origin`. |

Rules:

- **Both fields optional and independent.** A pre-`master_url` repo using only legacy `state_remote` lives in the new file shape without a synthetic `master_url`.
- **Empty pointer = standalone signal.** Both fields absent ⇒ standalone. `save()` of an empty pointer *removes the file* rather than writing `{}`, so a detached repo's `.balls/` carries no zero-content artifact.
- **`master_url` wins over `state_remote`** when both are set: the URL path materializes a state-repo; the name path does not.
- **Legacy fallback.** A pre-bl-82a4 repo kept these fields directly in `.balls/config.json`. `MasterPointer::load()` synthesizes a pointer from the in-canonical fields when no `master.json` exists, so the rest of `bl` never branches on file shape. Migration into the file shape happens on the next `bl remaster <url>`. The `Config` struct retains `master_url` / `state_remote` fields *for deserialization only* — they are explicitly **not read** by new code (`src/config.rs`).
- **The pointer is the one tracked federation artifact.** Everything else in the federated layout is gitignored and re-materializable (Principle 4).

## 6. State-repo materialization

`state_repo::ensure(root, url)` materializes `.balls/state-repo/` and is idempotent. It runs at `bl remaster` time and again, automatically, on the first `Store::discover` after a fresh `git clone` (the clone carries only the pointer — §8).

First-time:

1. `git init --initial-branch balls/tasks .balls/state-repo` and `git remote add origin <url>`. The state branch as the initial branch keeps the first orphan commit on the right ref with no separate checkout.
2. `git fetch origin`.
3. If `origin` has `balls/tasks`: create a tracking branch. Else: create the orphan locally and best-effort push it (a divergent hub rejects the non-force push; the repo stays safe-but-unlinked).
4. Seed `.balls/tasks/.gitattributes` (`*.notes.jsonl merge=union`) and a `.gitkeep`, committing if anything is uncommitted.
5. Materialize the three symlinks (§7).

Re-entry (state-repo already exists): re-point `origin.url` to the recorded URL (a later `bl remaster --commit` may have changed it), refresh the symlinks, fetch. No reseed.

**Re-materializability (Principle 4).** `.balls/state-repo/` is gitignored. Deleting it is safe: the next `Store::discover` re-runs `ensure` from the pointer and re-clones. The only unrecoverable loss is state-branch commits that exist *only* in the local state-repo and were never pushed to the hub — the same exposure as any unpushed git commit, not a new failure mode.

## 7. The symlink seam

In federated mode three paths under `.balls/` are symlinks into the state-repo:

| Project path | Target | Materialized by |
|---|---|---|
| `.balls/tasks` | `state-repo/.balls/tasks` | `ensure_tasks_symlink` |
| `.balls/plugins` | `state-repo/.balls/plugins` | `ensure_plugins_symlink` |
| `.balls/config.json` | `state-repo/.balls/config.json` | `ensure_config_symlink` |

**Why filesystem-shaped rather than code-branched.** The model decision is made *once*, at `bl remaster` time, by laying down symlinks. No call site that reads config or plugin files branches on `master_url` — `Config::load(".balls/config.json")` resolves to the hub's file transparently. The seam is the filesystem, not an `if`.

**Hand-editing still works (Principle 8).** Editing `.balls/config.json` through the project root transparently writes into `.balls/state-repo/.balls/config.json`; commit on `balls/tasks` inside the state-repo and push. Every participant sees it through its own symlink on the next `bl prime`.

Materialization is idempotent and defensive:

- `ensure_config_symlink` leaves a **real** `.balls/config.json` untouched — a standalone repo, or a legacy federated repo mid-migration. It only creates the symlink when the path is absent (the fresh-clone case) or repoints a stale symlink.
- `ensure_plugins_symlink` removes a placeholder (`.gitkeep`-only) `.balls/plugins/` dir, but **refuses** one carrying real plugin config files — promoting those into the hub is `bl remaster`'s job (§10), not a silent side effect.

## 8. Discovery and the `state_worktree_dir()` seam

`resolve_layout()` → `state_worktree_for(root)` is the single seam that picks the layout, resolved once at `Store` construction and cached:

```
state_worktree_dir = master_pointer.master_url().is_some()
                     ? root/.balls/state-repo      # federated
                     : root/.balls/worktree        # standalone
```

Every state-branch git operation — `create`, `claim`, `review`, `close`, `update`, `sync`, `reconcile` — runs in `store.state_worktree_dir()`. Routing the model choice through one cached path is what keeps the rest of `bl` model-agnostic.

`auto_provision_master()` runs on `Store::discover`: if the committed pointer sets `master_url` but `.balls/state-repo/.git` does not exist (the fresh-`git clone` case), it calls `state_repo::ensure`. This is what makes a teammate's onboarding "`git clone` + `bl prime`" with no extra step — the pointer bootstraps everything.

## 9. Reachability: hard-fail vs soft-fail

`state_repo::ensure` distinguishes two cases by whether a **warm cache** exists (the state-repo has a local `balls/tasks` branch):

- **First-time, hub unreachable** (no warm cache): **hard-fail.** Roll back the just-created scaffold so the next attempt is a clean first-time, and abort with a diagnostic that names the URL, the underlying `git fetch` stderr, and three resolution paths: fix access, edit the pointer, or `bl remaster --detach`.
- **Warm cache present, hub unreachable**: **soft-fail.** Work from the local cache, print a `note:` to stderr, continue. Parity with standalone offline operation.

The hard-fail is load-bearing (Principle 7). Silently materializing an empty local orphan when a first-time client cannot reach its hub would let that client accumulate tasks no teammate ever sees — drift, the exact failure the federated model exists to eliminate. A loud failure with an escape hatch is correct; a silent fallback is a spec violation.

The deadlock this creates — `master_url` set, state-repo never materialized, hub unreachable, so `Store::discover` itself hard-fails — is broken by the **cold detach** path (§11): `bl remaster --detach` must run *without* a successful `discover`.

## 10. Master-wins config policy

The federated flip (`bl remaster <url> --commit`, §11) **promotes** the project's policy up into the hub, then replaces it with symlinks:

1. Stash the project-side `.balls/config.json` and `.balls/plugins/` aside.
2. Materialize the state-repo and the symlinks.
3. **Merge project policy into the hub canonical, hub-wins:** a scalar field (`target_branch`, `delivery`, `min_bl_version`) travels up *only* if the hub left it unset. A project plugin entry is adopted *only* if the hub has no entry of that name; a name clash drops the project entry and the discarded names are printed in the command output. Bootstrap fields (`master_url`, `state_remote`) never travel into the canonical.
4. Commit the hub canonical onto the hub's `balls/tasks`.

After the flip there is exactly one config file and one plugins dir — the hub's — and every participant reads them through a symlink. "Hub wins" is therefore not a runtime rule applied on every read; it is a one-time migration followed by structural single-sourcing. `bl plugin enable|disable|policy` from any participant writes to the hub's `balls/tasks` (`bl sync` publishes); `bl plugin list|show` reports the source as `hub`.

This is what lets the bridge/proxy pattern (§12) keep credentials, sync schedule, and mirroring policy in one place across an N-participant federation: there is no project-side config for a participant to drift.

## 11. `bl remaster` — the lifecycle verb

`bl remaster` is the single verb for every model transition. Recovery and re-pointing must be first-class, not manual git surgery.

| Invocation | Effect |
|---|---|
| `bl remaster <url>` | Per-clone: materialize `.balls/state-repo/` from `<url>`. Not committed; no flip. |
| `bl remaster <url> --commit` | **Federated flip**: materialize, promote policy (§10), write + commit the pointer, gitignore + untrack the now-symlinked sidecars. Idempotent — re-running against the same hub refreshes the pointer. Also migrates a legacy in-canonical `master_url` repo into the file shape. |
| `bl remaster <name>` / `--commit` | **Legacy `state_remote`** (§13): reconcile against an existing project-side git remote by name. `--commit` writes the name into the pointer. |
| `bl remaster --detach` | Sever the hub link, return to standalone (warm or cold path below). Takes no target. |

`bl remaster <url> --commit` works on a **non-initialized** repo too (`bootstrap_non_initted`): a fresh `git clone` with no `.balls/` is a complete federation onboard in one command. A non-initted target *requires* `--commit` — the pointer must be tracked for `git clone` to carry it.

**Strand guard.** The URL flip adopts the hub's `balls/tasks` history wholesale; unlike the legacy name path's `reconcile`, it has no machinery to carry local-only tasks across. A *true standalone* repo with open local tasks is therefore refused the flip — closing or discarding them, or `--force` (which abandons them with a warning), is required. A repo already linked to a hub is never at risk and is not guarded.

**Reconcile** (legacy name path, `remaster::reconcile`): fetch the target's `balls/tasks`, replay local-only tasks on top of the hub history, and re-import id-clashing tasks under fresh ids (`bl create`'s id-retry primitive), remapping `parent` / `depends_on` / `links` / `closed_children` references. Idempotent: a target already an ancestor of local `balls/tasks` is a no-op.

**Detach** has two paths:

- **Warm** (`remaster::detach`): the state-repo has materialized. Re-root `balls/tasks` as a fresh local orphan carrying its current tasks; transplant it from `.balls/state-repo/` onto the project git's `.balls/worktree/` (the layout a post-detach `discover` resolves once `master_url` is cleared); restore `.balls/plugins/` as a real directory carrying the hub's files; discard `.balls/state-repo/` (a leftover is a re-federation footgun — `ensure` would key "warm cache" off it); turn `.balls/config.json` back into a real file; clear the pointer.
- **Cold** (`try_cold_detach`): `master_url` is set but the state-repo never materialized (the §9 deadlock). Runs with no successful `discover`, strictly locally, no network: clear the pointer, scrub legacy in-canonical fields, re-initialize the standalone `.balls/worktree/` layout on the project's own git.

Detach is offline-capable by construction (Principle 5): a repo is never trapped in a federation it cannot reach.

## 12. The `repo` field and the multi-repo operating model

The shared `balls/tasks` branch is flat — `bl prime` lists every ball across every participant. Per-repo legibility rests on one task field:

- **`repo`** (`Task.repo`, `Option<String>`): the `origin` URL of the code repo a ball belongs to. `bl create` stamps a provisional value; `bl claim` re-anchors it to the claiming clone — the definitive code home. Always a fetchable URL or null (`null` = "origin unknown", never a bare basename). Frozen-by-convention after claim, not locked: `bl update <id> repo=<url>` is the explicit fixup.
- **`delivered_repo`** (`Task.delivered_repo`): provenance of the delivery commit, for cross-repo `delivered_in` resolution when a ball is delivered in a repo other than the one resolving `bl show`.

Operating conventions (the authoritative *mechanics* are here; the worked guidance is SKILL.md): claim a ball from the clone whose code it touches, since the worktree lands under that clone; filter `bl ready` on the `repo` field rather than reinventing it as a tag; model cross-cutting work as one epic plus one child per repo.

**The bridge/proxy pattern.** A multi-repo project wires one external-tracker plugin into a single participant — the **bridge** — and lets the other repos operate through the shared state branch as a proxy. They never install the plugin, never hold its credentials, never run its sync. This is the §1 motivation-2 payoff, and it is *only* sound because of the master-wins single-sourcing (§10): mirroring policy lives in the hub config, so there is one place it can be set and no place it can drift. The bridge's `BALLS_IDENTITY` should be stable — it appears on every mirrored ball. This pattern is operational guidance; its diagram and rationale live in SKILL.md, and it is a *consequence* of this spec, not a separate mechanism.

## 13. `state_remote` (legacy) and its deprecation

`state_remote` predates `master_url`. It names an existing *project-side git remote* whose `balls/tasks` is negotiated against — workable for the single-repo "shared hub via an extra remote" case.

Its defect for the cross-repo case: a remote *name* is not self-bootstrapping. A fresh `git clone` lands without that remote in its `.git/config` and stays *safe but unlinked* until a teammate manually runs `git remote add <name> <url> && bl remaster <name>`. `master_url` has no such gap — the URL is in the committed pointer, so `git clone` + `bl prime` is a complete onboard.

`state_remote` is **deprecated for the cross-repo case as of 0.5.0** in favor of `master_url`. It is retained: read transparently via the pointer, migrated into the file shape on the next `bl remaster <url>`, and never auto-rewritten. Removal is out of scope for this spec.

## 14. Backwards-compatibility audit

The matrix: an old `bl` (pre-this-spec) and a new `bl` against standalone and federated repos.

| Scenario | Behavior | Risk | Mitigation |
|---|---|---|---|
| New `bl`, standalone repo (no pointer) | Resolves `.balls/worktree`; bit-identical to SPEC-orphan-branch-state.md | None | Principle 1; existing tests are the regression suite. |
| Old `bl`, federated repo | Old `bl` does not know `.balls/master.json`. It ignores the pointer and resolves the *standalone* `.balls/worktree` layout. | **Accepted caveat.** Old `bl` reads/writes a stale or empty local orphan, not the hub. | `min_bl_version` in the hub config warns a new-enough client; an old client cannot see it. Documented in README + SKILL. No engineering prevention — consistent with the project's 2026-05-10 decision on the deferred-mode caveat. |
| New `bl`, legacy in-canonical `master_url` (pre-bl-82a4) | `MasterPointer::load` synthesizes a pointer from the in-canonical config fields; resolves federated | None | `read_legacy`. Migrated into the file shape on the next `bl remaster <url>`. |
| Old `bl` reads a `Task` with `repo` / `delivered_repo` set | `Task` has no `deny_unknown_fields`; unknown fields round-trip through `extra` | None | Confirmed against `src/task.rs`; same lenient-serde guarantee SPEC-forge-gated-delivery.md §10 relies on. |
| New `bl`, federated repo, hub unreachable, first time | Hard-fail with rollback + remediation paths (§9) | None — loud by design | The escape hatch is `bl remaster --detach` (cold path). |
| New `bl`, federated repo, hub unreachable, warm cache | Soft-fail; works from cache; `note:` to stderr | None | Parity with standalone offline operation. |
| `bl remaster --detach` while offline | Cold path runs with no `discover`, no network | None | Principle 5; §11 cold path. |
| Federated repo, `git clone` by a teammate | Carries only the committed pointer; `auto_provision_master` re-materializes on first `discover` | None | §8. The gitignored scaffold is rebuilt, not cloned. |

The single accepted caveat is **old `bl` on a federated repo silently using a standalone layout**. It is documented and version-advised, not engineered against, mirroring the deferred-mode caveat.

## 15. Hand-operable shell sequences

Mirroring SPEC-orphan-branch-state.md §11. Every federated operation is stock git plus a symlink and a JSON edit.

```sh
# Stand up a bare hub from an existing balls-initialized project:
git clone --bare <proj-with-balls/tasks> hub.git   # carries balls/tasks

# Join a hub by hand (equivalent to `bl remaster <url> --commit`):
git -C proj clone --no-checkout <hub-url> .balls/state-repo   # balls-owned clone
git -C proj/.balls/state-repo checkout balls/tasks
ln -sf state-repo/.balls/tasks       proj/.balls/tasks
ln -sf state-repo/.balls/plugins     proj/.balls/plugins
ln -sf state-repo/.balls/config.json proj/.balls/config.json
printf '{\n  "master_url": "%s"\n}\n' "<hub-url>" > proj/.balls/master.json
# gitignore .balls/state-repo .balls/config.json .balls/plugins, then:
git -C proj add .balls/master.json .gitignore && git -C proj commit -m "balls: federate"

# Read federated state with no bl:  the symlinks make it transparent.
cat proj/.balls/tasks/bl-abcd.json | jq .
cat proj/.balls/config.json        | jq .

# Detach by hand:  drop the pointer, restore real files, re-root the orphan.
rm proj/.balls/master.json proj/.balls/config.json proj/.balls/plugins
# ...restore .balls/config.json + .balls/plugins/ as real files, re-root balls/tasks
# onto .balls/worktree per SPEC-orphan-branch-state.md §11.
```

These sequences must remain valid for the life of the design. A change that breaks them is a breaking change to this spec.

## 16. Non-goals

- **Per-cell / per-field merge of the shared store.** The hub is git; conflict resolution is the existing field-wise resolver on the text-mergeable schema. Federation does not add a merge engine.
- **A daemon, server, or sync service on the hub.** The hub is a passive git repo. Nothing runs on it.
- **Automatic discovery of which repo a ball targets.** `repo` is anchored at claim and is an explicit, hand-fixable field — not inferred from ball content.
- **N-way config merge.** "Hub wins" is single-sourcing, not reconciliation. Two hubs are two federations; a repo belongs to one.
- **Cross-hub task movement.** Moving a ball between two distinct hubs is out of scope — detach to standalone and re-federate is the supported path.
- **Securing the hub.** Access control is git's (SSH keys, repo permissions). balls adds no auth layer.

## 17. Open questions

1. **Stale-symlink detection in `bl doctor`.** `doctor` probes the federated symlinks today. Should it also detect a `.balls/state-repo/` whose `origin` has drifted from the pointer's `master_url` (e.g. hand-edited pointer, un-refreshed clone)? Leaning yes — a read-only drift check is exactly `doctor`'s job.
2. **`min_bl_version` enforcement for the old-`bl`-sees-standalone caveat (§14).** Advisory-only is consistent with the deferred-mode decision, but the failure here is quieter (wrong store, not a contaminated branch). Revisit if it bites a real federation.
3. **Garbage-collecting the hub's `balls/tasks` history.** Long-lived federations accumulate state-branch history across many repos. Out of scope here; flag if hub clone size becomes a problem.
4. **`repo`-scoped `bl ready` / `bl list`.** §12 says filter on `repo` by hand. Should `bl ready --repo <url>` be a first-class filter? Defer to a separate ball; it is additive and does not affect this spec's invariants.

## 18. Conformance tests

Each bullet corresponds to at least one integration test. No implementation ball lands until every test exists and fails for the right reason.

1. **Default unchanged.** A repo with no pointer: a full `bl create → claim → review → close` cycle is bit-identical to a standalone SPEC-orphan-branch-state.md repo, including resolved paths and commits.
2. **Pointer round-trip.** `MasterPointer` with `master_url` set saves, reloads equal; an empty pointer's `save()` removes the file; `is_empty()` is the standalone signal.
3. **Legacy fallback.** A `.balls/config.json` carrying in-canonical `master_url` and no `master.json`: `load()` synthesizes the pointer; the repo resolves federated.
4. **Federated flip.** `bl remaster <url> --commit` on a standalone, task-free repo: state-repo materialized, three symlinks created, pointer written + committed, sidecars gitignored + untracked, `git status` clean.
5. **Strand guard.** The flip on a standalone repo with ≥1 open local task is refused; `--force` proceeds with a warning; a repo already linked to a hub is not guarded.
6. **Policy promotion, hub-wins.** A project with `target_branch` set and a plugin entry, flipped against a hub that has a *different* value for each: the hub's scalar is kept, the project plugin entry is discarded and named in the output.
7. **Fresh-clone onboard.** `git clone` a federated repo (carries only the pointer), then `bl prime`: `auto_provision_master` materializes the state-repo and symlinks; `bl ready` lists hub tasks.
8. **Hard-fail first-time.** `master_url` set, hub unreachable, no warm cache: `state_repo::ensure` aborts, rolls back the scaffold, and the error names the URL + fetch stderr + three remediation paths.
9. **Soft-fail warm.** Same, but a warm cache exists: `bl` works from the cache and prints the `note:`; no abort.
10. **Cold detach.** `master_url` set, state-repo never materialized, hub unreachable: `bl remaster --detach` succeeds offline, clears the pointer, re-inits `.balls/worktree`.
11. **Warm detach.** A materialized federated repo: `bl remaster --detach` re-roots `balls/tasks` onto `.balls/worktree`, restores real `.balls/config.json` + `.balls/plugins/`, discards `.balls/state-repo/`, and the result resolves standalone.
12. **Reconcile (legacy name path).** `bl remaster <name>` replays local-only tasks onto the target history and re-imports id clashes under fresh ids with references remapped; a second run is `AlreadyUpToDate`.
13. **`repo` anchoring.** `bl create` stamps a provisional `repo`; `bl claim` re-anchors it to the claiming clone's `origin`; `bl update <id> repo=<url>` overrides it.
14. **Old-`bl` simulation.** A pre-spec binary on a federated repo resolves `.balls/worktree`, not the hub (fixture, not a live old-binary run) — documents the §14 accepted caveat.
15. **Hand-operability.** The §15 join sequence, executed with stock git + `ln`, produces a layout `Store::discover` resolves as federated.
