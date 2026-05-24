# SPEC: Workspace Layout — XDG dirs and Orphan-Branch Bootstrap

Status: draft
Scope: defines where a balls workspace's on-disk artifacts live and how `bl` discovers them on first contact. Pivots the layout so that **balls touches nothing on the code branch**: no committed config, no `.gitignore` insertion, no `.balls-worktrees/` colocated in-tree. Workspace config, state-repo checkouts, task worktrees, plugin auth, claims, and locks all relocate under XDG dirs in the user's home. The bootstrap fact becomes a well-known orphan branch name on `origin`, compiled into `bl`.

This SPEC supersedes the on-tree layout described in [SPEC-tracker-state.md](SPEC-tracker-state.md) §4 (the model), §5 (the address), §6 (materialization), §7 (config ownership location), and §13 (hand-operable sequences). The invariants in SPEC-tracker-state §2 carry forward unchanged; the realization changes. Where the two documents disagree on physical layout or read-order, this file wins.

It does not change the orphan-branch task state model ([SPEC-orphan-branch-state.md](SPEC-orphan-branch-state.md) §5 merge invariant, §6 delivery tag), nor the code/state split ([SPEC-tracker-state.md](SPEC-tracker-state.md) §10).

---

## 1. Motivation

The original design intent — stated in the bl-030e SPEC and restated as Principle 1 of [SPEC-orphan-branch-state.md](SPEC-orphan-branch-state.md) — was that a code repository's history is balls-clean. `git log main` contains feature commits and nothing else. The unstated corollary, latent in the same SPEC, was stronger: balls is *invisible* from the code repo's perspective. Clone the repo without `bl` installed, and you see nothing balls-shaped.

The current realization diverges from the corollary in three places:

- `.balls/config.json` is committed on `main` (bl-e609 deliberately put it there to make the workspace's tracker address readable before any other balls state).
- `.balls-worktrees/` colocates inside the workspace, requiring a `.gitignore` entry.
- The "what is tracked vs ignored under `.balls/`" mental model has to be carried by hand: `.balls/state-repo` gitignored, `.balls/config.json` tracked, `.balls/project.json` a symlink, `.balls/local/plugins/` ignored. The plugin migration ball (bl-de57) added a one-shot commit on `main` to clean up legacy committed plugin files — necessary, and itself an instance of the problem.

These are not mistakes. Each is correct given what preceded it:

- **bl-8a9a** unified the state checkout — retired the `master_url`-mode `.balls/state-repo` vs standalone `.balls/worktree` split into one checkout (`.balls/state-repo`). One mechanism, not a mode. That refactor had to land before "where does the checkout live" could be revisited as a single question.
- **bl-e609** split config ownership — workspace fields in `config.json` (committed on the code branch), project fields in `project.json` (committed on the tracker branch). That split had to land before workspace-owned config could be relocated independent of project-owned config; conflating them would have forced project config off the tracker too.
- **bl-de57** absorbed legacy committed plugin files into the unified state checkout and committed the cleanup on `main`. That one-shot was necessary scaffolding but is exactly the kind of `main` commit this SPEC's target shape prohibits: it succeeded *and* demonstrated that as long as anything balls-related lives on `main`, balls retains a pretext to write there.

Each step was the right move given its predecessor. The current state is a stepping stone, not the destination. This SPEC is the last step in a sequence that began with bl-030e: relocating the workspace's bootstrap fact from a tracked file to a well-known branch name, and relocating the runtime state from inside the repo to XDG dirs in the user's home. After this lands, balls writes to `main` exactly once — the migration commit (§11) — and never again.

## 2. Principles

The invariants from SPEC-tracker-state §2 hold without exception. Restated for cross-reference and to bind this SPEC's scope to them:

1. **The default is the model.** A repo with no redirect resolves to its own origin's `balls/tasks` branch and behaves bit-identically to the federated case where workspace and tracker happen to coincide. There is no "off" and no "standalone mode"; there is the default.
2. **One mechanism, not a mode.** XDG layout is the only layout. There is no `--classic` flag, no in-repo `.balls/` fallback. Once a workspace migrates, the in-repo layout ceases to exist on it.
3. **One VCS.** The tracker remains an ordinary git repo; every operation is `git fetch` / `git merge` / `git push` on the state branch. XDG dirs hold checkouts of that repo, nothing more.
4. **The bootstrap fact is a branch name, not a file path.** `bl` is compiled knowing that `origin balls/tasks` is always the first hop on any workspace. This SPEC strengthens the earlier "address lives in a file readable before anything else" formulation (SPEC-tracker-state §4 principle 4): the address no longer lives in a file at all. A workspace cannot rename or relocate this convention; only its *contents* are workspace-owned.
5. **Bilateral mobility.** Workspace identity is the origin URL. Re-cloning a workspace to a different directory or onto a different machine resolves the same workspace key and rebinds to the same on-disk state automatically. Federation redirects are still set with `bl remaster` and removed with `bl remaster --detach`; both are offline-capable.
6. **Ownership is split, and enforced.** Workspace-owned fields and project-owned fields live in disjoint files on disjoint branches. The split from bl-e609 is preserved; only the workspace-owned file relocates (§6). No field has two owners, so no precedence rule is needed for steady-state operation.
7. **Hard-fail first contact, soft-fail warm.** First-time materialization against an unreachable explicit tracker aborts loudly. A warm checkout works offline.
8. **Hand-operable.** A human with `git`, `ln`, `jq`, and `sha256sum` can stand up a workspace, join a tracker, read state, and detach (§10). The XDG paths are reproducible from the origin URL without consulting any bl-specific state.
9. **Merge cleanliness is unconditional.** Carries forward unchanged from SPEC-tracker-state §11.

One addition specific to this SPEC:

10. **Workspace identity is keyed by origin URL.** Two clones of the same code repo (different directories on disk, possibly different machines via a shared home) share the per-origin parts of the layout — the state-repo checkout cache, the tracker access — and isolate the per-clone parts — claims, locks, worktrees — under a deterministic `path-hash` subkey. Same scope as the current "workspace-owned = per-code-repo" semantics, just relocated and made explicit.

## 3. Layout

XDG-strict. No `~/.balls/` convenience collapse; the three dirs (config, state, cache) play their canonical roles.

```
~/.config/balls/<origin-key>/
  active/                            # symlinks into whichever state-repo is currently active
    project.json -> ../../../../.local/state/balls/<origin-key>/state-repos/<active-key>/.balls/project.json
    tasks        -> ../../../../.local/state/balls/<origin-key>/state-repos/<active-key>/.balls/tasks
    plugins      -> ../../../../.local/state/balls/<origin-key>/state-repos/<active-key>/.balls/plugins
  own/                               # symlinks into the workspace's own origin balls/tasks checkout
    workspace.json -> ../../../../.local/state/balls/<origin-key>/state-repos/own/.balls/workspace.json

~/.local/state/balls/<origin-key>/
  state-repos/
    own/                             # checkout of <origin-url> balls/tasks (workspace's own)
      .balls/
        workspace.json               # workspace-owned config + optional redirect (§6)
    <active-key>/                    # checkout of the federated tracker, when redirected
      .balls/
        tasks/                       # task files (the project state)
        project.json                 # project-owned config (§6)
        plugins/                     # plugin config (project-owned)
  <path-hash>/                       # per-clone, disk-isolated section
    claims/                          # claim files
    locks/                           # task locks
    plugins-auth/                    # tokens, identity (per-user, per-clone)
    worktrees/
      bl-xxxx/                       # task worktrees

~/.cache/balls/<origin-key>/         # derived/regenerable artifacts (e.g. resolved URL indices,
                                     # plugin caches). Empty in the initial layout; reserved.
```

`<origin-key>` and `<path-hash>` are explicit functions of inputs (§4); a human can compute them on the command line. The `active/` subdir is the hand-readable surface: `jq . ~/.config/balls/<origin-key>/active/project.json` works on any workspace.

`active/` and `own/` collapse to the same target in the solo case where the workspace's own `balls/tasks` *is* the tracker — `<active-key>` equals `own`. The symlink targets are then identical and the two sets of files coexist on the same branch at disjoint paths (`.balls/workspace.json` vs `.balls/project.json`). This is by design: the layout's shape does not change between solo and federated, only the resolved targets.

Nothing under the workspace's working tree is balls-shaped. No `.balls/`, no `.balls-worktrees/`, no `.gitignore` entries. A fresh `git clone` of the code repo shows zero balls footprint.

## 4. Workspace identity

Identity has two layers, keyed by inputs the user already has.

**`<origin-key>`** — derived from the workspace's `origin` remote URL. The URL string is canonicalized (lowercased scheme/host, trailing `.git` stripped, no trailing slash, no userinfo), then hashed with SHA-256, hex-encoded, and truncated to the first 16 characters. Example:

```
git@github.com:mudbungie/balls.git
  → canonical: github.com:mudbungie/balls
  → sha256(canonical) | hex | head -c 16 = e9f3a7c41b8d2056   (illustrative)
```

Two clones of the same origin — same machine, different directories; or different machines via a shared home — resolve the same `<origin-key>`. Their state-repo checkouts (`state-repos/own/`, `state-repos/<active-key>/`) and active-view symlinks are shared. Tracker access is shared too: one fetch refreshes both clones' view.

**`<path-hash>`** — derived from the absolute filesystem path of the workspace's git directory (`git rev-parse --absolute-git-dir`). Same hash function, same truncation. Used to isolate per-clone state — claims, locks, plugin auth, worktrees — so two clones of the same origin on the same machine do not collide. A moved workspace gets a new `<path-hash>` automatically.

A pre-remote workspace (one with no `origin` configured) cannot derive `<origin-key>`. This SPEC does not engineer around that: the bootstrap convention (§5) requires `origin` to exist. A solo project that wants balls before pushing anywhere uses `bl init` to create an initial origin (the offline-bootstrappable case is `bl init` with a local bare repo as origin).

## 5. Bootstrap convention

`bl` is compiled knowing one fact about every workspace: **the orphan branch `balls/tasks` on `origin` is always the first hop**. This is not a configuration value, not a discoverable file, not overridable per-workspace. It is a constant baked into the binary.

The contents of that branch determine what happens next:

1. **Branch absent.** The repo is uninitialized. `bl init` creates the branch with an initial `workspace.json` (no redirect) and pushes.
2. **Branch present, `workspace.json` carries no redirect.** This branch IS the tracker. Task files (`.balls/tasks/`), project config (`.balls/project.json`), and plugin config (`.balls/plugins/`) live on the same branch alongside `workspace.json`. Solo / non-federated case.
3. **Branch present, `workspace.json` carries `state_url` and/or `state_branch`.** Follow the redirect to the federated tracker. The tracker's `balls/tasks` branch carries the project's state (tasks, project.json, plugins) and is checked out into `state-repos/<active-key>/`.

The redirect is **single-hop**. A federated tracker's `project.json` may not itself carry `state_url`/`state_branch` — those are workspace-owned fields, not project-owned, and the project file's schema does not include them (§6). Cycle detection is unnecessary in the single-hop model but `bl` validates it as a defense-in-depth check: if any redirect ever resolves back into a chain longer than 1, abort with a "chained redirect detected" error. The depth limit is therefore 1, exactly, not a tunable.

That the bootstrap fact is a branch name has one consequence worth naming: a tracker cannot redirect a workspace. The branch name is fixed by the binary; only the redirect *contents* are tracker-owned. SPEC-tracker-state §2 principle 4 ("the address is the only bootstrap fact") survives in stronger form — the bootstrap fact is now a constant, not a value.

## 6. Workspace config and project config

Two files, two owners, no field shared between them. The bl-e609 split is preserved; only the workspace-owned file moves.

The files live at different paths to make the split structural rather than schema-only:

- **`.balls/workspace.json`** — on the workspace's own `origin balls/tasks` branch (`state-repos/own/.balls/workspace.json`). Workspace-owned.
- **`.balls/project.json`** — on the federated tracker's `balls/tasks` branch (`state-repos/<active-key>/.balls/project.json`). Project-owned. In the solo case, this file lives on the same branch as `workspace.json` but at a different path.

Field table:

| `.balls/workspace.json` — **workspace** | `.balls/project.json` — **project** |
|---|---|
| `state_url`, `state_branch` (the redirect; both absent ⇒ no redirect, solo case) | `version` (store schema) |
| `target_branch`, `delivery`, `review.pre_check` | `id_length` |
| `worktree_dir`, `protected_main` | `min_bl_version` |
| `require_remote_on_claim` / `_review` / `_close` | `plugins` (config + participant policy) |
| `auto_fetch_on_ready`, `stale_threshold_seconds` | |

The split rationale is unchanged from SPEC-tracker-state §7: workspace fields describe how *this code repo and clone* integrate; project fields describe shared backlog policy. Plugin *auth* (tokens, `BALLS_IDENTITY`) lives under `~/.local/state/balls/<origin-key>/<path-hash>/plugins-auth/` — neither file, per-clone, per-user, never committed.

Because the fields are disjoint per file, there is no overlap-and-precedence rule to apply per-read. Each field has one owner and lives in exactly one place. The migration-precedence text in SPEC-tracker-state §7 is retained only for the cross-version transition where an older `bl` may have written a project-owned field into the workspace file — see the backwards-compat audit (§12).

The field names of the redirect (`state_url`, `state_branch`) are preserved from SPEC-tracker-state §5 so that `bl remaster` semantics carry over unchanged. The file the redirect lives in is what moved.

## 7. Materialization

`Store::discover` is the entry point. Idempotent; runs on every `bl` invocation. Steps:

1. **Derive `<origin-key>`.** Read `origin.url` from the workspace's `.git/config`, canonicalize, hash, truncate (§4). If `origin` is missing, abort with the "no origin configured" diagnostic.
2. **Ensure the XDG tree.** Create `~/.config/balls/<origin-key>/`, `~/.local/state/balls/<origin-key>/`, `~/.cache/balls/<origin-key>/` as needed. Each is a plain `mkdir -p`; safe under concurrent invocations.
3. **Fetch the workspace's own `balls/tasks`.** Single-branch fetch into `~/.local/state/balls/<origin-key>/state-repos/own/`. First contact clones; subsequent invocations fetch and fast-forward. If the tracker is unreachable, §9 of SPEC-tracker-state decides (the rule carries unchanged: hard-fail explicit, soft-fail warm).
4. **Read `workspace.json`.** Parse `state-repos/own/.balls/workspace.json`. If `state_url`/`state_branch` are absent, set `<active-key> = own` and skip to step 6. Otherwise continue.
5. **Fetch the federated tracker.** Compute `<active-key>` from the redirect's `state_url` (same canonicalize-and-hash). Clone or fetch `state_branch` into `state-repos/<active-key>/`. Single-hop only; if the tracker's `project.json` contains `state_url` or `state_branch`, abort.
6. **Materialize the active view.** Create `~/.config/balls/<origin-key>/active/` and place three symlinks pointing into `state-repos/<active-key>/.balls/`: `tasks`, `project.json`, `plugins`. Create `~/.config/balls/<origin-key>/own/workspace.json` symlink pointing into `state-repos/own/.balls/workspace.json`. Replace stale symlinks; never error on existing correct ones.
7. **Derive `<path-hash>` and ensure the per-clone tree.** Compute from the workspace's git directory (§4). Create `~/.local/state/balls/<origin-key>/<path-hash>/{claims,locks,plugins-auth,worktrees}/` as needed.

Reachability rules (SPEC-tracker-state §9) and merge cleanliness (SPEC-tracker-state §11) carry forward unchanged. The state-repo checkouts are the same git repos as before, just relocated.

The whole layout is regenerable from `(origin URL, workspace path)`. Delete `~/.local/state/balls/<origin-key>/` and the next `bl` invocation rebuilds it. The only unrecoverable loss is state-branch commits never pushed to the tracker — the ordinary exposure of any unpushed git work.

## 8. Worktree relocation

Task worktrees live at `~/.local/state/balls/<origin-key>/<path-hash>/worktrees/bl-xxxx/`. `bl claim` invokes `git worktree add` with an absolute off-tree path; the workspace's `.git` tracks the worktree as it would any other.

One accepted ergonomic cost: a worktree is no longer colocated under the workspace's working tree, so `ls .balls-worktrees/` does not enumerate active claims. The replacement read surface is `bl list` (which already enumerates claims by status) and `ls ~/.local/state/balls/<origin-key>/<path-hash>/worktrees/`. The trade was named in the parent epic and accepted.

A moved workspace breaks the per-clone state binding: the `<path-hash>` changes, so the old `worktrees/` and `claims/` are orphaned at the old hash. `bl doctor` detects this on read by enumerating sibling `<path-hash>` subdirs under `<origin-key>/` and matching their recorded workspace path against the current one. On finding orphaned state, `doctor` reports the old path, the new path, the orphaned task IDs, and offers a `bl repair --rebind-path` command to move them. Read-only by default; `--fix` adds the action. No automatic rebind.

## 9. Code/state split

SPEC-tracker-state §10 carries forward unmodified. `bl review` squashes the work branch onto the *workspace's own* integration branch on the *workspace's own* `origin`; the `[bl-xxxx]` delivery tag lands there. Only the state-branch transition reaches the tracker.

This relocation does not touch code delivery. `repo` (the code repo a task belongs to) and `delivered_repo` (where the delivery commit landed) remain on the task file. The constraint that `bl show` in repo C can resolve a task delivered in repo B only if it can reach B is unchanged.

The pre-XDG layout already enforced this split; moving the workspace's runtime state into XDG dirs strengthens the separation by removing the last in-repo balls artifact — the `.balls/config.json` file — that gave the impression of a hybrid model. Post-XDG, there is no shared file on the code branch, so no read can mistakenly conflate code-branch state with tracker state.

## 10. Hand-operability

The §13 sequences of SPEC-tracker-state are replaced. The full clone-and-join sequence:

```sh
# Inputs: ORIGIN=<workspace origin URL>; TRACKER=<federated tracker URL or empty>

# 1. Derive origin-key (same function bl uses):
canonical=$(echo "$ORIGIN" | sed -E 's#^[a-z]+://##; s#^[^@]+@##; s#\.git$##; s#/$##' | tr 'A-Z' 'a-z')
origin_key=$(printf '%s' "$canonical" | sha256sum | head -c 16)

# 2. Ensure XDG dirs:
mkdir -p ~/.config/balls/$origin_key/active ~/.config/balls/$origin_key/own
mkdir -p ~/.local/state/balls/$origin_key/state-repos
mkdir -p ~/.cache/balls/$origin_key

# 3. Clone the workspace's own balls/tasks (the bootstrap branch):
git clone --single-branch --branch balls/tasks "$ORIGIN" \
    ~/.local/state/balls/$origin_key/state-repos/own

# 4. Always: own/workspace.json points at the workspace's own checkout.
ln -sfn ../../../../.local/state/balls/$origin_key/state-repos/own/.balls/workspace.json \
    ~/.config/balls/$origin_key/own/workspace.json

# 4a. Solo case (no redirect): the active checkout IS the own checkout.
ln -sfn ../../../../.local/state/balls/$origin_key/state-repos/own/.balls/tasks \
    ~/.config/balls/$origin_key/active/tasks
# ... and similarly for project.json, plugins.

# 4b. Federated case: read the redirect, clone the tracker, then symlink.
state_url=$(jq -r '.state_url // empty' ~/.local/state/balls/$origin_key/state-repos/own/.balls/workspace.json)
state_branch=$(jq -r '.state_branch // "balls/tasks"' ~/.local/state/balls/$origin_key/state-repos/own/.balls/workspace.json)
if [ -n "$state_url" ]; then
    canonical_t=$(echo "$state_url" | sed -E 's#^[a-z]+://##; s#^[^@]+@##; s#\.git$##; s#/$##' | tr 'A-Z' 'a-z')
    active_key=$(printf '%s' "$canonical_t" | sha256sum | head -c 16)
    git clone --single-branch --branch "$state_branch" "$state_url" \
        ~/.local/state/balls/$origin_key/state-repos/$active_key
    # symlink active/* into state-repos/$active_key/.balls/*
fi

# 5. Per-clone tree:
path_hash=$(git -C <workspace-path> rev-parse --absolute-git-dir | sha256sum | head -c 16)
mkdir -p ~/.local/state/balls/$origin_key/$path_hash/{claims,locks,plugins-auth,worktrees}

# 6. Read state with stock git + jq:
jq . ~/.config/balls/$origin_key/active/tasks/bl-abcd.json
git -C ~/.local/state/balls/$origin_key/state-repos/own log balls/tasks
```

These sequences must remain valid for the life of the design. A change that breaks them — the canonicalization, the hash function, the directory shape — is a breaking change to this SPEC.

## 11. Migration

A pre-XDG workspace has, in some combination:

| Pre-XDG artifact | Destination | Action |
|---|---|---|
| `.balls/config.json` on `main` (committed) | `state-repos/own/.balls/workspace.json` | `bl migrate` writes the file on the workspace's own `balls/tasks` branch, then `git rm` on `main` |
| `.balls/state-repo/` (gitignored runtime checkout of the tracker) | `state-repos/<active-key>/` (or `state-repos/own/` if solo) | `bl migrate` moves the checkout; or simply re-fetches into the XDG path and removes the old one |
| `.balls/tasks`, `.balls/project.json`, `.balls/plugins` symlinks | (none) | Removed; the new view lives under `~/.config/balls/<origin-key>/active/` |
| `.balls/local/` (claims, locks, plugin auth) | `~/.local/state/balls/<origin-key>/<path-hash>/{claims,locks,plugins-auth}/` | `bl migrate` copies contents; old dir removed |
| `.balls-worktrees/bl-xxxx/` (active task worktrees) | `~/.local/state/balls/<origin-key>/<path-hash>/worktrees/bl-xxxx/` | `bl migrate` uses `git worktree move`; refuses if any worktree has uncommitted changes (caller is expected to commit or drop first) |
| `.gitignore` entries (`.balls/state-repo`, `.balls-worktrees`, etc.) | (none) | Removed in the migration commit |
| `.balls/` directory at workspace root | (none) | Removed in the migration commit |

The migration commit on `main` carries the message `balls: migrate to XDG layout [bl-xxxx]` and is the final balls-attributed commit on `main` for the lifetime of the workspace. After it lands, `bl` writes to `main` only via `bl review`'s squash-merges (which carry feature commit messages, not `balls:`-prefixed bookkeeping).

`bl migrate` is idempotent: re-running on an already-migrated workspace is a no-op (it observes that no `.balls/` exists at the workspace root and nothing on `main` needs to be removed). It refuses to run if the workspace has uncommitted changes on `main` or in any task worktree — the migration commit is too disruptive to silently merge into in-flight work.

Migration is one-way. There is no `bl unmigrate`. A workspace that wants the pre-XDG layout back reverts the migration commit by hand and runs `bl detach`; this is a manual recovery path, not a supported workflow. The non-goal in §13 rules out a `--classic` toggle.

## 12. Backwards-compatibility audit

Carries over the row-by-row analysis of SPEC-tracker-state §12, updated for the relocated layout:

| Scenario | Behavior | Risk | Mitigation |
|---|---|---|---|
| New `bl`, repo with no balls history | `bl init` creates `balls/tasks` on `origin` with a default `workspace.json` (no redirect); XDG dirs materialize on first `discover`. No commit on `main`. | None | Phase 0 default; conformance test §14.1. |
| New `bl`, pre-XDG repo with `.balls/config.json` on `main` and `.balls/state-repo/` runtime | Phase 1 (dual-read) reads both layouts and prefers the new one; if the new one is absent, falls back to the old. Phase 2 (`bl migrate`) is opt-in until Phase 3, when `bl prime --migrate` becomes the default. | A workspace that runs Phase-1 `bl` for a long period accumulates state in both layouts and stays correct, but the in-repo `.balls/` does not shrink until migration | Phase 1 emits a one-line "legacy layout in use" warning; migration is one-shot and well-defined. |
| Pre-XDG `bl` on a post-XDG repo (post-migration) | Pre-XDG `bl` looks for `.balls/config.json` on the working tree, finds nothing, and reports "not a balls repo." It cannot misroute because it cannot find a starting point. | Pre-XDG agents stop being able to use the workspace until they upgrade | Documented, version-advised via `min_bl_version` on the workspace's `balls/tasks` branch. Old `bl` does not know how to read `min_bl_version` from a branch it cannot find, so the warning channel is "the upgrade message in the project tracker." Accepted: the same documented-not-engineered caveat from SPEC-tracker-state §12, in stronger form (silent misroute is impossible because the binding is broken, not soft-redirected). |
| New `bl`, pre-XDG repo with `.balls/plugins/*.json` committed on `main` (pre-bl-de57) | bl-de57's one-shot self-migration absorbs plugin files into the runtime checkout and commits cleanup on `main`. After this SPEC's migration, those files relocate again into `state-repos/<active-key>/.balls/plugins/`. The migration commit covers both transitions. | A user who never ran bl-de57's fix and migrates straight to XDG could lose plugin config | `bl migrate` detects committed plugin files on `main` and runs bl-de57's absorb step as part of the migration; one commit, two transitions. |
| Workspace moved on disk (post-XDG) | `<origin-key>` unchanged; `<path-hash>` changes; per-clone state at the old hash is orphaned. `bl doctor` enumerates and offers `bl repair --rebind-path`. | Orphaned worktrees at the old hash are invisible to `bl list`; `git worktree list` in the workspace's `.git` still references them and is the canonical recovery point | `doctor` is the user-facing detector; the underlying `git worktree` state is intact. |
| Origin URL changed (e.g. moved from GitHub to GitLab) | `<origin-key>` changes; all workspace state is orphaned at the old key. Equivalent to a fresh clone against the new origin. | All in-flight task worktrees stranded | `bl doctor` detects via `.git/config`'s prior `origin` URL (if recorded) and offers a one-shot `bl repair --rebind-origin <old> <new>`. Manual recovery if `doctor` cannot reconstruct the old URL. |
| Two clones of the same origin on the same machine, concurrent operation | Share `<origin-key>` (so share the state-repo cache and tracker access — one fetch refreshes both) but isolate `<path-hash>` (so claims, locks, worktrees do not collide). | None | Conformance test §14.3. |
| Shared `$HOME` across machines (NFS, sync) | Same `<origin-key>` resolves to the same XDG paths. `<path-hash>` is per-host because git directories differ. State-repo checkouts shared across hosts; per-host claims/locks isolated. | NFS-style `$HOME` is rare among balls users but technically supported; concurrent fetches into the shared state-repo could race | Treat the same as two clones on one machine for the per-clone layer; for the shared layer, rely on git's own `index.lock` to serialize fetches. |

The accepted caveat is the third row: pre-XDG `bl` on a post-XDG workspace fails-loud instead of misrouting. This is *better* than the SPEC-tracker-state §12 caveat (which was silent misroute) — the relocation makes engineering around it unnecessary because the old binary cannot find a starting point. Version-advised via `min_bl_version` is retained as the proactive channel.

## 13. Non-goals

The non-goals from SPEC-tracker-state §14 carry forward without change:
- A daemon, server, or sync service on the tracker.
- A custom merge engine; conflict resolution remains the field-wise resolver on the text-mergeable schema.
- N-way config reconciliation.
- Cross-tracker task movement.
- Securing the tracker; access control remains git's.
- Per-workspace overrides of project-owned fields.

Added by this SPEC:

- **No in-repo `.balls/` fallback.** The migration is one-way (idempotent forward, manual reverse via `git revert` on `main` plus `bl detach`). There is no `--classic` toggle, no `BALLS_CLASSIC_LAYOUT` env var, no per-workspace "use the old layout" config. The XDG layout is the layout.
- **No `$XDG_*` collapse to `~/.balls/`.** Users with non-standard `$XDG_CONFIG_HOME` / `$XDG_STATE_HOME` / `$XDG_CACHE_HOME` are honored. A single `~/.balls/` would be simpler but it is not XDG, and the XDG spec is what the rest of the user's tooling assumes.
- **No per-workspace overrides of `<origin-key>` derivation.** The canonicalization and hash function are constants in the binary. Users who clone the same repo via two different URLs (HTTPS vs SSH) get two `<origin-key>`s; the recommended fix is to normalize the remote URL with `git remote set-url`, not to engineer a URL-alias layer into balls.

## 14. Conformance tests

Numbered list of behaviors the test suite must gate. Each test must (a) exist before its corresponding implementation ball lands and (b) fail for the right reason against pre-implementation code. The pattern is the same as SPEC-tracker-state §16: new-side assertions, not pinned old-binary fixtures.

1. **XDG paths from a fresh `bl init`.** A workspace initialized with `bl init` has no `.balls/` directory at the workspace root; on first `bl prime` the `~/.config/balls/<origin-key>/active/`, `~/.local/state/balls/<origin-key>/state-repos/own/`, and `~/.local/state/balls/<origin-key>/<path-hash>/worktrees/` paths exist and are correctly populated. No commit on `main` is created by `bl init` or `bl prime`.
2. **Origin-key derivation.** Given a fixed list of input URLs (HTTPS, SSH, with/without `.git`, with/without trailing slash, mixed case), the canonicalization and hash produce the documented `<origin-key>` values. Frozen golden vectors so a future implementation cannot drift.
3. **Per-clone isolation.** Two clones of the same origin into different directories share `state-repos/own/` (same `<origin-key>`) but have distinct `<path-hash>` subdirs; concurrent `bl claim` of two different tasks from the two clones writes to disjoint claim files and does not deadlock or corrupt either clone's locks.
4. **Bootstrap branch is constant.** `bl` does not consult any file at any path to determine the bootstrap branch name. Mutating `.balls/config.json` or `.balls/workspace.json` to override the branch name has no effect — the binary fetches `origin balls/tasks` regardless.
5. **Redirect — single hop only.** A `workspace.json` with `state_url` resolves to one federated tracker checkout. If that tracker's `project.json` contains a `state_url` field (synthetic test fixture; the schema does not allow it in normal operation), `bl prime` aborts with the "chained redirect" error.
6. **Disjoint schemas.** `workspace.json` and `project.json` parsed against their schemas have empty intersection of field names. An unknown field in either round-trips via `extra` per the existing forward-compat invariant (bl-1b07: symmetric unknown=round-trip across serde seams).
7. **Materialization is idempotent.** Running `bl prime` twice produces a `~/.config/balls/<origin-key>/` tree with identical inode-level state on the symlinks; no churn, no rebuilds, no error.
8. **State-repo regenerability.** Deleting `~/.local/state/balls/<origin-key>/state-repos/` and running `bl prime` rebuilds the checkouts from the tracker; only state-branch commits never pushed are lost (and only those — the test asserts pushed state survives).
9. **Worktrees relocated.** `bl claim bl-xxxx` creates a worktree at `~/.local/state/balls/<origin-key>/<path-hash>/worktrees/bl-xxxx/`. The workspace's working tree contains no `.balls-worktrees/` directory and `.gitignore` contains no balls-related entries.
10. **`bl doctor` detects moved workspace.** A workspace whose `<path-hash>` no longer matches recorded per-clone state surfaces an orphaned-claims report; `bl repair --rebind-path` moves the per-clone state to the new hash with no data loss.
11. **No on-`main` writes (steady state).** Across a full lifecycle (`bl init`, `bl create`, `bl claim`, `bl review`, `bl close`) on a post-migration workspace, `git log main` gains exactly the `[bl-xxxx]` delivery commit from `bl review` — and nothing else. No `balls: *` commits.
12. **Hand-operable join sequence.** The §10 shell sequence, executed with stock `git`, `ln`, `jq`, and `sha256sum`, produces a layout that subsequent `bl` invocations accept and operate on without further setup.
13. **Migration is one-shot.** Running `bl migrate` on a pre-XDG workspace produces (a) the documented XDG tree, (b) one commit on `main` removing the in-repo `.balls/` and `.gitignore` entries, and (c) no further state in the workspace's working tree. Re-running `bl migrate` is a no-op.
14. **Pre-XDG fallback never reappears (§12 caveat, new-side).** New `bl` on a post-migration workspace never creates `.balls/` in the workspace working tree, never reads from `.balls/config.json` on `main`, and never writes to `.balls-worktrees/` — the exact behaviors a pre-XDG binary, ignorant of the new layout, would exhibit. Asserted on the new side rather than as an old-binary fixture: per SPEC-tracker-state §16.13, an old binary's behavior is immutable and a pinned-old fixture passes against pre-spec code and so cannot fail-then-pass across the refactor as the gating pattern requires. The new-side contrapositive does both — it fails against pre-XDG code (which still does write to those paths) and documents the caveat by absence.
15. **`min_bl_version` advisory.** On a workspace whose `project.json` carries `min_bl_version > current`, `bl prime` prints a one-line upgrade warning and continues. (The warning lives on the project file, which a sufficiently old `bl` cannot read; this test asserts the warning emits for new `bl` running against a newer-`min_bl_version` project — the proactive channel works for upgraders.)

No phase ball under bl-f021 lands until its corresponding conformance tests exist and fail for the right reason against pre-implementation code.
