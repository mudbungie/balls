# SPEC: Tracker State — Address, Checkout, and Config Ownership

Status: draft
Scope: defines where a balls project's task state lives, how `bl` checks it out, and who owns its configuration. Replaces the `master_url` "federated mode" that shipped incrementally (bl-ffb4 … bl-82a4) with a single un-moded model. A repo that has always used the defaults is unaffected — the defaults *are* the model's degenerate case.

This is the authoritative contract. README §"Multi-repo" and SKILL.md are operational summaries; where they disagree with this file, this file wins. It supersedes the `.balls/worktree` checkout described in [SPEC-orphan-branch-state.md](SPEC-orphan-branch-state.md) §4/§8/§10 (see §4 below); SPEC-orphan-branch-state.md §5's merge invariant it depends on and restates (§11).

## 1. Motivation

The orphan-branch design ([SPEC-orphan-branch-state.md](SPEC-orphan-branch-state.md)) welds the task store to the code repo: `balls/tasks` is an orphan ref in the project's own git, checked out as a worktree. One repo, one task store. That is correct for a solo project.

It does not fit a project spanning several code repositories. A product with a `frontend`, an `api`, and an `infra` repo wants **one** backlog — one ready queue, cross-repo epics, cross-repo dependency edges. Under the orphan-branch model each repo has its own disjoint `balls/tasks`; `frontend`'s `bl ready` cannot see a ball filed from `api`. There is no shared upstream because the state ref has no remote of its own — it rides the code remote.

Federation shipped as a bolt-on: a `master_url` *mode* with its own discovery branch, a second checkout mechanism (`.balls/state-repo` beside the standalone `.balls/worktree`), a config-symlink seam, and a `master.json` pointer file. A design review found the intent sound but the realization fragmented: two code paths for one job, a `bl remaster --detach` that must physically transplant the orphan between the two layouts, and a config file whose ownership was ambiguous.

This spec unifies it. **There is no federated mode.** There is a *tracker address* — always present, with an implicit default — and `bl` materializes a checkout of it. "Standalone" is the case where the address points at the code repo's own origin; "federated" is the case where it points at a shared tracker. One mechanism, one checkout, one code path; the address is the only thing that varies.

## 2. Principles

Invariants. An implementation that violates one is out of scope; revisit the spec instead.

1. **The default is the model.** A repo with no tracker address configured resolves the implicit default — its own `origin`, branch `balls/tasks` — and behaves bit-identically to a pre-federation repo. There is no "off"; there is the default. Existing tests are the regression suite.
2. **One mechanism, not a mode.** Standalone and federated are one code path differing only in the address value. No `if federated` branch in discovery, no second checkout type, no transplant on detach.
3. **One VCS.** The tracker is an ordinary git repo. No database, no daemon, no service. Every tracker operation is `git fetch` / `git merge` / `git push` on the state branch.
4. **The address is the only bootstrap fact.** It lives in the workspace's own committed `config.json`. Everything else — the `.balls/state-repo` checkout, the symlinks — is gitignored runtime state, re-materialized from the address by `Store::discover`. Deleting it loses nothing not already pushed.
5. **Bilateral mobility.** A repo moves between addresses (`bl remaster`) with no task-data loss, and `bl remaster --detach` — pointing the address back at self — must work offline. A repo is never trapped in a tracker it cannot reach.
6. **Ownership is split, and enforced.** Config has two owners: the *workspace* owns how this repo builds and integrates; the *project* owns the shared backlog policy. They live in two files (§7). The project file wins on overlap, so project ownership cannot be silently overridden by a stale workspace value. This is not a merge and not a layering to reason about per-read — each field has one owner.
7. **Hard-fail first contact, soft-fail warm.** First-time materialization against an unreachable *explicit* tracker aborts loudly. Once a warm checkout exists, an unreachable tracker is a soft-fail (work from the checkout). The implicit-default address may always fall back to a local checkout — a solo project is offline-bootstrappable.
8. **Hand-operable.** The layout is a `git clone` plus three `ln -s`. A human with `git`, `ln`, and `jq` can stand up a tracker, join it, read state, and detach — §13.
9. **Merge cleanliness is unconditional.** Disjoint-field edits to a task merge clean under stock git, always — no "common case," no field exempt (§11). The conformance suite gates it.

## 3. Terminology

- **tracker**: the git repo hosting the shared `balls/tasks` branch. In a solo project this is the code repo's own origin; in a multi-repo project it is a dedicated, usually code-free repo. (Distinct from an **external tracker** — Jira, Linear, GitHub Issues — which a plugin mirrors to. When this document says "tracker" unqualified it means the balls tracker.)
- **workspace**: the repo where `bl` runs and from which task worktrees (`.balls-worktrees/<id>/`) span. The recommended deployment, the README's "bare workspace", is one of these with `core.bare = true` (older docs called it the "bare central hub" — same concept).
- **tracker address**: the pair `(state_url, state_branch)` — where the state branch lives and what it is named. Implicit default: `(origin, balls/tasks)`.
- **state-repo**: `.balls/state-repo/` — the balls-owned checkout of the tracker address. The single checkout, standalone or federated.
- **repo config**: `.balls/config.json` — workspace-owned, committed to the workspace's code branch.
- **project config**: `.balls/project.json` — project-owned, committed on the tracker branch, inherited by every workspace.

## 4. The model

One repo resolves one layout. The layout is identical standalone and federated; only `state_url` differs.

```
workspace/
├── .git/                       the code repo's git (code origin untouched)
├── .balls/
│   ├── state-repo/             checkout of the tracker address — gitignored
│   │   └── .balls/
│   │       ├── tasks/          task files (the state branch tree)
│   │       ├── project.json    project config
│   │       └── plugins/        plugin config
│   ├── tasks        -> state-repo/.balls/tasks         symlink
│   ├── project.json -> state-repo/.balls/project.json  symlink
│   ├── plugins      -> state-repo/.balls/plugins       symlink
│   ├── config.json             repo config — a REAL file, committed to the code branch
│   └── local/                  per-workspace ephemeral: claims, locks, plugin auth
└── .balls-worktrees/           task worktrees
```

`state_url` resolves to the code repo's `origin` (solo) or a dedicated tracker (multi-repo). Nothing else moves: `bl`, an agent, and the README's hand sequences see the same paths either way. The orphan-branch SPEC's `.balls/worktree` — a worktree of the project's own git — is replaced by `.balls/state-repo`, an independent checkout of the address. One consequence is honest and accepted: `git log balls/tasks` from the *project root* no longer resolves (the project's `.git` need not carry the ref at all); the blessed path is `git -C .balls/state-repo log`, which the orphan-branch hand sequences already used. In exchange, the project's own history is *guaranteed* free of `balls:` commits — orphan-branch Principle 1, strengthened from convention to structure.

## 5. The tracker address

The address lives in the workspace's `config.json` as two optional fields:

| Field | Absent ⇒ | Set ⇒ |
|---|---|---|
| `state_url` | the code repo's `origin`, resolved live | an explicit tracker URL |
| `state_branch` | `balls/tasks` | an explicit branch name |

Both absent is the implicit default `(origin, balls/tasks)` — a standalone repo's `config.json` carries neither field, which is why a pre-federation config is already conformant. `bl remaster <url>` writes `state_url`; `bl remaster --detach` removes it. `state_branch` lets one tracker host several projects on distinct branches, or a workspace point at a non-default branch; it is referenced exactly as any branch name.

The address must be readable *before* anything else resolves, which is why it lives in `config.json` — a plain, never-symlinked file — and not in the project config (which is reached *through* the address). The retired `master.json` pointer existed only because the old model symlinked `config.json` to the tracker; with `config.json` workspace-owned and never symlinked (§7), the address simply folds in. A repo carrying the legacy `master.json`, or the older in-`config.json` `master_url`/`state_remote` fields, is migrated to `state_url`/`state_branch` on the next `bl remaster` and read transparently until then.

## 6. Materialization

`Store::discover` materializes `.balls/state-repo/` from the address. Idempotent; runs on first discover after a fresh `git clone` (the clone carries `config.json` with the address, nothing else).

1. **First contact.** Clone `state_branch` from `state_url` into `.balls/state-repo` (single-branch). If the tracker is reachable but has no `state_branch`, create it as an orphan and push. If the tracker is unreachable, §9 decides.
2. **Warm.** `.balls/state-repo` exists: fetch, fast-forward, done.
3. **Seed.** Ensure the state branch carries `.balls/tasks/.gitattributes` (`*.notes.jsonl merge=union`), an empty `project.json` if absent, and `plugins/`.
4. **Symlinks.** Materialize `.balls/tasks`, `.balls/project.json`, `.balls/plugins` pointing into `.balls/state-repo`. `.balls/config.json` is left alone — it is a real workspace file.

`.balls/state-repo` is gitignored and re-materializable: delete it and the next `discover` rebuilds it from the address. The only unrecoverable loss is state-branch commits never pushed to the tracker — the ordinary exposure of any unpushed git work, and the reason the tracker, not the local checkout, is the durable home (Principle 4).

## 7. Config ownership

Two files, two owners, no field owned by both:

| `config.json` — **workspace** (committed to the code branch) | `project.json` — **project** (committed on the tracker branch) |
|---|---|
| `state_url`, `state_branch` (the tracker address) | `version` (store schema) |
| `target_branch`, `delivery`, `review.pre_check` | `id_length` |
| `worktree_dir`, `protected_main` | `min_bl_version` |
| `require_remote_on_claim` / `_review` / `_close` | `plugins` (config + §11 participant policy) |
| `auto_fetch_on_ready`, `stale_threshold_seconds` | |

The split follows ownership, not convenience. `review.pre_check` is the proof case: a Rust tracker's `make check` cannot gate a JavaScript participant, so the build/test gate is *workspace* property. `target_branch` and `delivery` describe how *this code repo* integrates — workspace. `id_length` must be consistent for every task ID minted into the shared store — project. Plugin config describes the project's mirror to an external tracker — project.

`project.json` is read through the `.balls/project.json` symlink, so a workspace always has it (the tracker hosts it even for a solo project, on the repo's own `balls/tasks`). **On overlap, `project.json` wins.** That precedence is load-bearing: it makes project ownership *enforced* — a stale or rogue `config.json` value for a project-owned field is ignored, not honored — and it is the migration path: move a field from `config.json` to `project.json`, new clients converge on the project value, old clients that do not know `project.json` fall back to their built-in default. A field never needs to live in both; the precedence exists for the transition and for integrity, not as a steady state.

**Plugins.** Plugin *config* (`.balls/plugins/*.json` — which external tracker, project key, status maps) is project config: it rides the tracker branch so a fresh clone inherits it. Plugin *auth* (tokens, under `.balls/local/plugins/`) and the worker's `BALLS_IDENTITY` are the user's — per-workspace, gitignored, never on the tracker. Running the plugin remains one workspace's job (the bridge, SKILL.md §proxy); the *config* being project-owned is what lets the bridge role move without reconfiguration.

`require_remote_*` stay workspace-owned: "I consider a task done when it merges on origin" is a per-repo workflow stance. The project-level counterpart — "this task is not done until the external tracker is updated" — is expressed as the plugin's `required` participant policy, which is in `plugins` and therefore project-owned. Two requirements, two owners, no overlap.

## 8. `bl remaster`

`bl remaster` is the one verb for every address change. It does not move task data between mechanisms — there is one mechanism — it rewrites the address and reconciles.

| Invocation | Effect |
|---|---|
| `bl remaster <url> [--branch B]` | Write `state_url` (`state_branch`) into `config.json`; re-materialize `.balls/state-repo`; reconcile local-only tasks onto the new tracker's history, renaming any id clashes. Idempotent. |
| `bl remaster --detach` | Remove `state_url`/`state_branch` from `config.json` (address reverts to the implicit default — the code `origin`); re-root the state branch as needed. Offline-capable: a workspace is never trapped. |

Reconcile replays the workspace's local-only tasks on top of the target history and re-imports id-clashing tasks under fresh ids (`bl create`'s id primitive), remapping `parent` / `depends_on` / `links` / `closed_children`. A target already an ancestor of the local state branch is a no-op.

Because there is no second layout, there is no transplant: `--detach` is an address edit plus an ordinary reconcile, and a re-`remaster` to a new tracker is the same. The old model's first-federation race — two repos seeding a fresh tracker — is likewise not a special case: the loser of the push is an ordinary diverged checkout, healed by the same reconcile path as any divergence.

## 9. Reachability

First contact (`.balls/state-repo` does not yet exist) against an unreachable tracker:

- **Explicit address** (`state_url` is set): **hard-fail.** Abort with a diagnostic naming the URL, the underlying `git` error, and the resolutions: fix access, edit `config.json`, or `bl remaster --detach`. A silent local orphan would let the workspace's task changes drift from the project — the failure the tracker exists to prevent.
- **Implicit default** (`state_url` absent — a solo repo, possibly pre-remote): fall back to a local `git init` of `.balls/state-repo`. A solo project is offline-bootstrappable; it publishes on the first reachable `bl sync`.

Once `.balls/state-repo` exists (warm), an unreachable tracker is a **soft-fail**: `bl` works from the local checkout and prints a note. Parity with ordinary offline git.

## 10. Code delivery vs. task state

A federation invariant, stated explicitly because it is easy to get wrong: **federation routes task state to the tracker; it does not route code.** `bl review` squashes the work branch onto the *workspace's own* integration branch and pushes to the *workspace's own* `origin`; the `[bl-xxxx]` delivery tag lands there. Only the state-branch transition goes to the tracker. The two remotes are independent and may be entirely unrelated repos.

This is why a task carries `repo` (the code repo it belongs to, anchored at `bl claim`) and `delivered_repo` (where its delivery commit landed). It also bounds a limitation honestly: [SPEC-orphan-branch-state.md](SPEC-orphan-branch-state.md) §6 makes the `[bl-xxxx]` tag ground truth — but the tag lives on the *delivering* repo's branch. `bl show` in repo C resolves a task delivered in repo B only if it can reach B. The tag is ground truth *within its delivering repo*; `delivered_repo` names that repo; cross-repo resolution requires reachability. There is no mechanism to change this without breaking the tag's backwards compatibility, and none is proposed — it is documented, not engineered around.

## 11. Merge cleanliness

Federation does not touch the merge model. Merges run in `.balls/state-repo` via stock `git merge` plus the field-wise resolver, exactly as [SPEC-orphan-branch-state.md](SPEC-orphan-branch-state.md) §5 specifies: one compact line per top-level task field ⇒ disjoint-field edits never collide, same-field edits are a genuine conflict for `bl sync`'s resolver. This holds unconditionally — every field, no "common case," no exemption.

The distinction the conformance suite must keep: **correctness is structural, smoothness is configuration.** Two workers closing *different* tasks merge clean always (different files); closing the *same* task is resolved always (status precedence). That is correct even fully offline. `require_remote_on_close` and its siblings do not make federation *correct* — they make it *smooth*, shrinking the divergence window so the resolver runs less often. A federation that wants closes to round-trip immediately turns those on; a federation that does not is still correct, just merges more.

## 12. Backwards-compatibility audit

| Scenario | Behavior | Risk | Mitigation |
|---|---|---|---|
| New `bl`, repo with no address fields | Resolves the implicit default `(origin, balls/tasks)`; bit-identical to pre-federation | None | Principle 1; existing tests are the regression suite. |
| New `bl`, repo with legacy `master.json` or in-`config.json` `master_url` | Read transparently; migrated to `state_url` on next `bl remaster` | None | §5 legacy migration. |
| Old `bl`, repo with `state_url` set in `config.json` | Old `bl` ignores the unknown field, resolves the standalone `.balls/worktree` of its own git | Old `bl` operates a stale local store, not the tracker | `min_bl_version` (project config) warns a new-enough client; an old client cannot see it. Documented, not engineered against — consistent with the 2026-05-10 decision on the deferred-mode caveat. |
| Old `bl` reads `project.json` / a task with `repo`, `delivered_repo` | Does not know `project.json`; `Task` has no `deny_unknown_fields` so unknown fields round-trip via `extra` | Old `bl` uses built-in defaults for project-owned fields | Acceptable: project policy degrades to defaults, task data is preserved. |
| New `bl`, fresh `git clone` of a federated workspace | `config.json` carries the address; `discover` materializes `.balls/state-repo` and the symlinks | None | §6. The gitignored checkout is rebuilt, not cloned. |
| New `bl`, pre-bl-8a9a repo with `.balls/plugins/*.json` (and `.gitkeep`) committed to the code branch | First discover absorbs the plugin files into `.balls/state-repo/.balls/plugins/` *and* commits `balls: migrate plugins to state checkout` on the code branch — `git rm`s the legacy index entries and refreshes `.gitignore` for the unified runtime paths in one commit | A pre-bl-de57 `bl` performed the absorb but neither cleanup, leaving `.balls/plugins/*` shown as deleted, `.balls/state-repo` untracked (an embedded-repo hazard on `git add -A`), and a stale `.gitignore` | bl-de57 self-migrates on first discover; pre-fix workspaces self-heal on the same trigger (both cold and warm paths); the migration commit is idempotent and a no-op on an already-clean workspace, so the parallel-discover storm stays out of the `index.lock` race. |
| `bl remaster --detach` offline | Address edit + reconcile, no network | None | Principle 5; §8. |

The single accepted caveat is an old `bl` on a workspace whose address points elsewhere: it silently operates its own git's `balls/tasks` instead of the tracker. Version-advised via `min_bl_version`, not prevented.

## 13. Hand-operable sequences

Every operation is stock git plus a symlink and a JSON edit.

```sh
# Stand up a dedicated tracker from a balls-initialized repo:
git clone --bare <repo-with-balls/tasks> tracker.git    # carries balls/tasks

# Join a tracker by hand (equivalent to `bl remaster <url>`):
git clone --single-branch --branch balls/tasks <tracker-url> ws/.balls/state-repo
ln -sf state-repo/.balls/tasks        ws/.balls/tasks
ln -sf state-repo/.balls/project.json ws/.balls/project.json
ln -sf state-repo/.balls/plugins      ws/.balls/plugins
# add state_url to ws/.balls/config.json; gitignore .balls/state-repo et al.

# Read state with no bl — the symlinks make it transparent:
jq . ws/.balls/tasks/bl-abcd.json
jq . ws/.balls/project.json

# Inspect task-state history:
git -C ws/.balls/state-repo log balls/tasks

# Detach: drop state_url from config.json, drop the symlinks, re-root.
```

These must remain valid for the life of the design. A change that breaks them is a breaking change to this spec.

## 14. Non-goals

- **A daemon, server, or sync service on the tracker.** The tracker is a passive git repo.
- **A merge engine.** Conflict resolution is the field-wise resolver on the text-mergeable schema (§11). Federation adds none.
- **N-way config reconciliation.** "Project wins" is single-sourcing with a deterministic overlap rule, not a merge.
- **Cross-tracker task movement.** A workspace belongs to one tracker; moving between two is detach-then-remaster.
- **Securing the tracker.** Access control is git's (SSH keys, repo permissions). balls adds no auth layer.
- **Per-workspace overrides of project-owned fields.** If a repo needs different backlog policy, it is a different project. Ownership is not negotiable per-clone.

## 15. Open questions

1. **Naming of `.balls/state-repo`.** It is accurate (a checkout of the tracker) but "state-repo" predates the "tracker" term. Reconsidered names — `.balls/tracker`, `.balls/state` — risk confusing the local checkout with the remote tracker. Left as `state-repo` pending a naming pass.
2. **`require_remote_*` as project-settable.** §7 keeps them workspace-owned. A federation may want to *mandate* synced closes rather than hope each workspace opts in. Adding a project-level default (workspace may tighten, not loosen) is additive and does not affect these invariants — defer to its own ball.
3. **`bl doctor` drift checks.** `doctor` should detect a `.balls/state-repo` whose `origin` has drifted from `config.json`'s `state_url`, and a `project.json` symlink that does not resolve. Additive; specify in the doctor ball.
4. **Tracker-branch history growth.** A long-lived multi-repo tracker accumulates state-branch history. No GC is specified; flag if checkout size becomes a problem.

## 16. Conformance tests

balls today implements federation as a `master_url` *mode* — a separate discovery branch, a `.balls/worktree`-vs-`.balls/state-repo` split, a symlinked `config.json`, a `master.json` pointer. This spec is the unified *target*; the tests below gate the refactor balls under the parent epic. Each must exist and fail against today's code before the corresponding refactor lands.

1. **Default is the model.** A repo with no address fields: a full `create → claim → review → close` cycle is bit-identical to a pre-federation repo, including resolved paths and commits.
2. **One checkout.** `Store::discover` resolves `.balls/state-repo` for both a default-address and an explicit-`state_url` repo — no `.balls/worktree` branch, no mode flag.
3. **Address round-trip.** `bl remaster <url>` writes `state_url` to `config.json`; `--detach` removes it; absent fields resolve to `(origin, balls/tasks)`.
4. **Fresh-clone onboard.** `git clone` a workspace carrying only `config.json` with `state_url`; `bl prime` materializes `.balls/state-repo` and the three symlinks; `bl ready` lists tracker tasks.
5. **Config ownership.** A project-owned field set in both `config.json` and `project.json` resolves to the `project.json` value; a workspace-owned field is read from `config.json`; `project.json` is reached through the symlink.
6. **Plugin split.** Plugin config rides the tracker branch and a fresh clone inherits it; `.balls/local/plugins/` auth is never written to the tracker.
7. **Hard-fail explicit / local-fallback default.** First contact with an unreachable explicit `state_url` aborts with the three-way diagnostic; first contact with no `state_url` set `git init`s `.balls/state-repo` locally.
8. **Soft-fail warm.** A materialized `.balls/state-repo` with an unreachable tracker works from the checkout and prints the note.
9. **Detach offline.** `bl remaster --detach` with the tracker unreachable succeeds, reverts the address, leaves a working standalone store.
10. **Reconcile.** `bl remaster <url>` replays local-only tasks onto the target history and re-imports id clashes under fresh ids with references remapped; a second run is a no-op.
11. **Code/state split.** In a federated workspace, `bl review` squashes onto the workspace's own `origin` and the `[bl-xxxx]` tag lands there; only the state transition reaches the tracker.
12. **Merge cleanliness, federated topology.** Two participants editing *disjoint* fields of the same task merge clean with zero conflicts; editing the *same* field surfaces a genuine conflict for the resolver; concurrent note appends merge clean under `merge=union`. Run in the two-participant federated topology, unconditionally.
13. **Old-`bl` caveat (§12).** A workspace with `state_url` set routes task state to the *tracker*; a new binary never falls back to its own git's `balls/tasks`, which is exactly what a pre-spec binary, ignorant of the field, would do. Asserted on the new side, not as an old-binary fixture: the §12 caveat is *documented, not engineered against*, an old binary's behaviour is immutable and outside this codebase, and a fixture running a pinned pre-spec binary would be invariant — passing against pre-spec code, it could never fail-then-pass across the refactor as this section requires. The new-side contrapositive does both: it fails against pre-spec code and documents the caveat by contrast.
14. **Hand-operability.** The §13 join sequence, run with stock git and `ln`, produces a layout `Store::discover` accepts.

No refactor ball lands until its conformance tests exist and fail for the right reason.
