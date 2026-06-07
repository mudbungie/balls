# SPEC: Clone Layout — XDG dirs, Orphan-Branch Bootstrap, Three-File Config

Status: draft. Fourth revision of the on-disk layout SPEC. The earlier revisions (bl-ed32 commit aa26cc0, bl-dfd1 commit 500b657, bl-fc50 commit 6e713d5) all keyed identity in two layers via SHA-256-truncated hashes — `<origin-key>` from the canonical origin URL, `<path-hash>` from the absolute git-dir path — and produced opaque paths like `~/.local/state/balls/e9f3a7c4.../4a2c8f1d.../worktrees/bl-abcd/`. This revision (bl-e9c2) drops the hashing entirely: every per-host path is now a nested transparent assembly of percent-encoded URL/branch components and the literal clone filesystem path. See §15 for the revision history.

Scope: defines where a balls clone's on-disk artifacts live and how `bl` discovers them on first contact. Pivots the layout so that **balls touches nothing on the code branch**: no committed config, no `.gitignore` insertion, no `.balls-worktrees/` colocated in-tree. Repo config, state-repo checkouts, task worktrees, plugin auth, claims, and locks all relocate under XDG dirs in the user's home. The bootstrap fact becomes a well-known orphan branch name on `origin`, compiled into `bl`.

This SPEC supersedes the on-tree layout described in [SPEC-tracker-state.md](SPEC-tracker-state.md) §4 (the model), §5 (the address), §6 (materialization), §7 (config ownership location), and §13 (hand-operable sequences). The invariants in SPEC-tracker-state §2 carry forward unchanged; the realization changes. Where the two documents disagree on physical layout or read-order, this file wins.

It does not change the orphan-branch task state model ([SPEC-orphan-branch-state.md](SPEC-orphan-branch-state.md) §5 merge invariant, §6 delivery tag), nor the code/state split ([SPEC-tracker-state.md](SPEC-tracker-state.md) §10).

---

## 1. Motivation

The original design intent — stated in the bl-030e SPEC and restated as Principle 1 of [SPEC-orphan-branch-state.md](SPEC-orphan-branch-state.md) — was that a code repository's history is balls-clean. `git log main` contains feature commits and nothing else. The unstated corollary, latent in the same SPEC, was stronger: balls is *invisible* from the code repo's perspective. Clone the repo without `bl` installed, and you see nothing balls-shaped.

The current realization diverges from the corollary in three places:

- `.balls/config.json` is committed on `main` (bl-e609 deliberately put it there to make the clone's tracker address readable before any other balls state).
- `.balls-worktrees/` colocates inside the clone, requiring a `.gitignore` entry.
- The "what is tracked vs ignored under `.balls/`" mental model has to be carried by hand: `.balls/state-repo` gitignored, `.balls/config.json` tracked, `.balls/project.json` a symlink, `.balls/local/plugins/` ignored. The plugin migration ball (bl-de57) added a one-shot commit on `main` to clean up legacy committed plugin files — necessary, and itself an instance of the problem.

These are not mistakes. Each is correct given what preceded it:

- **bl-8a9a** unified the state checkout — retired the `master_url`-mode `.balls/state-repo` vs standalone `.balls/worktree` split into one checkout (`.balls/state-repo`). One mechanism, not a mode. That refactor had to land before "where does the checkout live" could be revisited as a single question.
- **bl-e609** split config ownership — repo fields in `config.json` (committed on the code branch), project fields in `project.json` (committed on the tracker branch). That split had to land before repo-owned config could be relocated independent of project-owned config; conflating them would have forced project config off the tracker too.
- **bl-de57** absorbed legacy committed plugin files into the unified state checkout and committed the cleanup on `main`. That one-shot was necessary scaffolding but is exactly the kind of `main` commit this SPEC's target shape prohibits: it succeeded *and* demonstrated that as long as anything balls-related lives on `main`, balls retains a pretext to write there.

Each step was the right move given its predecessor. The current state is a stepping stone, not the destination. This SPEC is the last step in a sequence that began with bl-030e: relocating the clone's bootstrap fact from a tracked file to a well-known branch name, and relocating the runtime state from inside the repo to XDG dirs in the user's home. After this lands, balls writes to `main` exactly once — the migration commit (§11) — and never again.

The three earlier revisions of this SPEC (bl-ed32, bl-dfd1, bl-fc50) keyed identity through SHA-256 hashes of the origin URL and absolute git-dir path. That hashing was the only mechanism by which the layout was not hand-readable: a path like `~/.local/state/balls/e9f3a7c4.../4a2c8f1d.../worktrees/bl-abcd/` cannot be read back to a clone or a repo without consulting bl. It also forced the SPEC to invent a "synthetic origin-key" fallback for stealth/no-git clones, which was the smell that the layering was over-engineered. This revision drops the hashing entirely: the URL is percent-encoded into one path component, the branch is percent-encoded into one path component, and the clone path is nested literally. The layout becomes navigable with `cd` and `ls` alone.

## 2. Principles

The invariants from SPEC-tracker-state §2 hold without exception. Restated for cross-reference and to bind this SPEC's scope to them:

1. **The default is the model.** A repo with no redirect resolves to its own origin's `balls/tasks` branch and behaves bit-identically to the federated case where clone and tracker happen to coincide. There is no "off" and no "standalone mode"; there is the default.
2. **One mechanism, not a mode.** XDG layout is the only layout. There is no `--classic` flag, no in-repo `.balls/` fallback. Once a clone migrates, the in-repo layout ceases to exist on it.
3. **One VCS.** The tracker remains an ordinary git repo; every operation is `git fetch` / `git merge` / `git push` on the state branch. XDG dirs hold checkouts of that repo, nothing more.
4. **The bootstrap fact is a branch name, not a file path.** `bl` is compiled knowing that `origin balls/tasks` is always the first hop on any clone. A clone cannot rename or relocate this convention; only its *contents* are repo-owned.
5. **Bilateral mobility.** Repo identity is the origin URL. Re-cloning a repo onto another directory rebinds its tracker access to the same `trackers/<enc-origin>/<enc-balls-tasks>/` checkout, automatically. Federation redirects are still set with `bl remaster` and removed with `bl remaster --detach`; both are offline-capable.
6. **Ownership splits across three config files plus one pointer file, each named for its layer's scope, with one shadowing rule for the fields that legitimately layer.** `project.json` (tracker-scope), `repo.json` (per-code-repo), `clone.json` (per-on-disk-checkout) — three config files. `tracker.json` (pointer-only) — the redirect signal. Layered fields obey **specific wins**: `clone.json ?? repo.json ?? project.json ?? built-in default`. Tracker-scope fields appearing outside `project.json` are rejected on read (§6).
7. **Hard-fail first contact, soft-fail warm.** First-time materialization against an unreachable explicit tracker aborts loudly. A warm checkout works offline.
8. **Hand-operable.** A human with `git`, `ln`, `jq`, and a percent-encoder (`jq -Rr @uri` will do) can stand up a clone, join a tracker, read state, and detach (§10). The XDG paths are reproducible from the origin URL and the clone's filesystem path without consulting any bl-specific state.
9. **Merge cleanliness is unconditional.** Carries forward unchanged from SPEC-tracker-state §11.

Two additions specific to this SPEC:

10. **Paths are transparent.** No hashing anywhere. Identity comes from the natural names: the canonical origin URL, the branch name, the clone's filesystem path. The first two are percent-encoded into one path component each; the third is nested literally with its leading `/` dropped. Every path on disk is recoverable back to its inputs by eye, by `xdg-open`, and by stock shell tools.
11. **One file has one parent.** Each on-disk artifact has exactly one path determined by its inputs. Two clones of the same code repo into different directories produce disjoint per-clone trees; two `bl init`s at the same path produce the same path. The layout has no aliases.

## 3. Layout

XDG-strict. No `~/.balls/` convenience collapse; the three dirs (config, state, cache) play their canonical roles.

```
~/.local/state/balls/
  trackers/<enc-origin>/<enc-balls-tasks>/   # state-repo checkout (per origin URL + branch)
    .balls/
      tasks/                                  # task files (when this IS the tracker)
      project.json                            # per-project config (when this IS the tracker)
      plugins/                                # plugin config (when this IS the tracker)
      repo.json                               # per-code-repo config (when this branch is a code repo's own)
      tracker.json                            # redirect pointer — present only on a redirecting code repo
  worktrees/<nested-clone-path>/<bl-id>/      # task worktrees (one per claimed task per clone)
                                              # e.g. worktrees/home/mark/dev/balls/bl-abcd/
  claims/<nested-clone-path>/                 # claim files (per clone)
  locks/<nested-clone-path>/                  # task locks (per clone)
  plugins-auth/<nested-clone-path>/           # plugin tokens, identity (per clone, per user)

~/.config/balls/<nested-clone-path>/
  clone.json                                  # per-on-disk-checkout override; present only when set

~/.cache/balls/                               # derived/regenerable artifacts. Empty in this layout; reserved.
```

`<enc-origin>` is the canonicalized origin URL percent-encoded into one path component (§4). `<enc-balls-tasks>` is `balls%2Ftasks` (the orphan branch name, percent-encoded). `<nested-clone-path>` is the clone's absolute filesystem path with the leading `/` dropped — e.g. `/home/mark/dev/balls` becomes `home/mark/dev/balls`. All three are deterministic functions of inputs the user already has on the command line.

`trackers/<enc-origin>/<enc-balls-tasks>/.balls/` is a checkout of the branch. In the **solo** case (no redirect), it carries the whole shape — `tasks/`, `project.json`, `plugins/`, and `repo.json` — coexisting on the same branch at distinct paths under `.balls/`, with no `tracker.json`. In the **federated** case the code repo's own checkout carries `repo.json` and `tracker.json`; the federated tracker's checkout at `trackers/<enc-tracker-origin>/<enc-balls-tasks>/.balls/` carries `tasks/`, `project.json`, and `plugins/`. The presence-or-absence of `tracker.json` is the structural difference. Reading the active state is *always* a combination: `repo.json` from the code repo's own checkout; `tasks` / `project.json` / `plugins` from whichever checkout is current (own in solo, redirected in federated).

Nothing under the clone's working tree is balls-shaped. No `.balls/`, no `.balls-worktrees/`, no `.gitignore` entries. A fresh `git clone` of the code repo shows zero balls footprint.

## 4. Clone identity

Identity has no derived keys. Three natural identifiers, each used directly:

**`<enc-origin>`** — the clone's `origin` remote URL, canonicalized then percent-encoded into a single path component. Canonicalization (same rules as previous revisions): lowercased scheme/host, trailing `.git` stripped, no trailing slash, no userinfo. Percent-encoding (RFC 3986 unreserved + percent encoding for everything else): `/`, `:`, `@`, `?`, `#`, `&`, spaces, and any non-ASCII byte are encoded as `%XX`. The result is reversible and contains no slashes, so the URL becomes one component:

```
git@github.com:mudbungie/balls.git
  → canonical: github.com:mudbungie/balls
  → percent-encoded: github.com%3Amudbungie%2Fballs
```

Two clones of the same origin produce the same `<enc-origin>` and share the same `trackers/<enc-origin>/<enc-balls-tasks>/` checkout — exactly the per-repo state sharing the previous revisions called "repo layer of identity." Their tracker fetches are shared on the host; one fetch refreshes both clones' view.

**`<enc-branch>`** — the orphan branch name percent-encoded. `balls/tasks` becomes `balls%2Ftasks`. This makes the branch a single path component (so a multi-slash branch name does not fan out into a nondeterministic subtree, and so `..` in a foreign string cannot escape).

**`<nested-clone-path>`** — the clone's absolute git-directory path (`git rev-parse --absolute-git-dir`'s parent for non-bare; the bare directory for bare), with the leading `/` stripped and slashes preserved. `/home/mark/dev/balls` becomes `home/mark/dev/balls`. This is hand-operable: `cd ~/.local/state/balls/worktrees/$(pwd | sed 's|^/||')/` from the clone root takes you to its worktree directory.

Two clones of the same origin on the same machine into different directories share the per-tracker state (`<enc-origin>` collides) and isolate the per-clone state (`<nested-clone-path>` does not). A moved clone gets a new `<nested-clone-path>` automatically.

A pre-remote clone (no `origin` configured) has no `<enc-origin>`. The bootstrap convention (§5) requires `origin` to exist for the non-stealth path. The stealth path bypasses this — see §4.1.

### 4.1 Stealth / no-git

Stealth mode is a clone with no `origin` (and possibly no git repository at all). `bl init --stealth [--tasks-dir <path>]` writes a `clone.json` with `stealth: true` and `tasks_dir: <path>` (defaulting to the current working directory's `.balls/tasks/`). Subsequent `bl` invocations from that working directory read `clone.json` first; on `stealth: true` they take `tasks_dir` as the task store directly and skip the entire origin / `trackers/` / `tracker.json` / redirect machinery.

A stealth clone's `<nested-clone-path>` is its canonical absolute filesystem path — the absolute `--tasks-dir` if one was given, otherwise the absolute working directory at `bl init --stealth` time. This puts `clone.json` at `~/.config/balls/<nested-clone-path>/clone.json` under the same scheme as a regular clone. There is no special-case in the layout; only the read-time short-circuit (clone.json → stealth → done) differs.

## 5. Bootstrap convention

`bl` is compiled knowing one fact about every non-stealth clone: **the orphan branch `balls/tasks` on `origin` is always the first hop**. This is not a configuration value, not a discoverable file, not overridable per-clone. It is a constant baked into the binary.

The contents of that branch determine what happens next:

1. **Branch absent.** The repo is uninitialized. `bl init` creates the branch with a default `repo.json` and a default `project.json`, no `tracker.json`, and pushes.
2. **Branch present, `.balls/tracker.json` absent.** This branch IS the tracker. Task files (`.balls/tasks/`), per-project config (`.balls/project.json`), plugin config (`.balls/plugins/`), and per-code-repo config (`.balls/repo.json`) all live on the same branch. Solo / non-federated case.
3. **Branch present, `.balls/tracker.json` present.** Follow the redirect to the federated tracker. The file is *pointer-only*: it carries exactly `state_url` and/or `state_branch`, nothing else. The tracker's `balls/tasks` branch carries the project's state (tasks, project.json, plugins) and is checked out into `trackers/<enc-state-url>/<enc-state-branch>/`. `repo.json` continues to live on the repo's own branch.

The redirect is **single-hop**. A federated tracker may not itself carry a `.balls/tracker.json` on its `balls/tasks` branch — the file's only legal location is on a code repo's *own* `balls/tasks` branch. Cycle detection is unnecessary in the single-hop model but `bl` validates it as a defense-in-depth check: if any redirect ever resolves into a chain longer than 1, abort with a "chained redirect detected" error. The depth limit is therefore 1, exactly, not a tunable.

That the bootstrap fact is a branch name has one consequence worth naming: a tracker cannot redirect a clone. The branch name is fixed by the binary; only the redirect *contents* are repo-owned (they sit on the repo's own branch). SPEC-tracker-state §2 principle 4 ("the address is the only bootstrap fact") survives in stronger form — the bootstrap fact is now a constant, not a value.

## 6. Config files — tracker.json, project.json, repo.json, clone.json

Four files, four owners. Three are config (`project.json`, `repo.json`, `clone.json`) with a shadowing rule for fields that legitimately layer; one is a pointer (`tracker.json`) with no schema overlap.

| file | location | scope | when present |
|---|---|---|---|
| `tracker.json` | on the code repo's own `balls/tasks` at `.balls/tracker.json` | redirect pointer | only when this repo redirects |
| `project.json` | on the tracker's `balls/tasks` at `.balls/project.json` (solo: same branch as `repo.json`) | tracker-scope — describes the shared task store's shape | always |
| `repo.json` | on the code repo's own `balls/tasks` at `.balls/repo.json` | per-code-repo — how this code repo integrates | always |
| `clone.json` | uncommitted, at `~/.config/balls/<nested-clone-path>/clone.json` | per-on-disk-checkout — local override | only when present |

### 6.1 `tracker.json` — pointer-only

Schema:

```json
{
  "state_url": "<url>",
  "state_branch": "<branch-name>"   // optional; defaults to "balls/tasks"
}
```

That is the whole schema. No other fields are accepted. Implementations reject extra fields on read with a "tracker.json schema: unexpected field" error rather than round-tripping them, because the file is so small that an unexpected field is almost certainly a stale write from an older binary or a hand-edit gone wrong.

The file's *presence* is the bootstrap signal (§5). If the file is absent on a repo's own branch, the branch is the tracker; the redirect contents are simply not asked for. Writing `tracker.json` with no `state_url` is illegal (it would be a `tracker.json` that says "redirect to nowhere"); `bl remaster` either writes a real pointer or removes the file.

The field names `state_url` and `state_branch` are preserved from SPEC-tracker-state §5 so that `bl remaster` semantics carry over unchanged.

### 6.2 `project.json` — tracker-scope config

`project.json` lives on the tracker's `balls/tasks` branch (in the solo case, the same branch as `repo.json`). It is the file that carries everything that varies *per project* — equivalently *per tracker*, *per shared task store*. The phrase "describes the shared task store's shape" is the load-bearing one: every federating code repo reads from the same `project.json`, so any field whose value would have to agree across the federation lives here.

Schema:

```json
{
  "version":         "<store schema>",
  "id_length":       4,
  "min_bl_version":  "0.4.0",
  "plugins":         { /* plugin config + participant policy */ },

  // Optional project-wide defaults for layered fields.
  // clone.json beats repo.json beats project.json per §6.5.
  "integrate":                 { "mode": "direct" },
  "review":                    { "gate_command": null },
  "require_remote_on_claim":   true,
  "require_remote_on_review":  true,
  "require_remote_on_close":   true
}
```

The first block — `version`, `id_length`, `min_bl_version`, `plugins` — is **tracker-scope-owned**. These describe the store's shape and cannot be overridden per-repo or per-clone (their primary-owner column in §6.4 is `project.json` only). The second block is **project-wide defaults** for fields whose primary owner is `repo.json`; they layer per §6.5.

### 6.3 `repo.json` — per-code-repo config

`repo.json` lives on the code repo's own `balls/tasks` branch (always; same branch as `tracker.json` if redirecting). It carries everything that varies *per code repo*: how `bl review` integrates work into the integration branch, what command gates review, what protections this code repo has on `main`, where worktrees go, which remote-round-trip policies are in force.

Schema (field-by-field rationale in §6.4):

```json
{
  "integrate":                 { "mode": "direct" },         // "direct" | "forge-pr"
  "review":                    { "gate_command": null },     // optional shell command; null disables
  "require_remote_on_claim":   true,
  "require_remote_on_review":  true,
  "require_remote_on_close":   true,
  "auto_fetch_on_ready":       true,
  "stale_threshold_seconds":   86400,
  "worktree_dir":              null,
  "protected_main":            true
}
```

All fields are optional; an absent field reads as its default (defaults in §6.4). A repo with no balls-specific configuration ships a zero-keys `repo.json` and gets the defaults.

A tracker-scope field (`version`, `id_length`, `min_bl_version`, `plugins`) appearing in `repo.json` aborts the read with a "tracker-scope field in repo.json" diagnostic (§6.5).

### 6.4 `clone.json` — per-on-disk-checkout override

`clone.json` is **never committed**. It lives in the user's XDG config at `~/.config/balls/<nested-clone-path>/clone.json`. It exists only when something at this on-disk checkout legitimately differs from the repo or project defaults.

Schema:

```json
{
  "stealth":   false,        // see §4.1; when true, tasks_dir is also required
  "tasks_dir": null,         // absolute path to the task store; only meaningful when stealth=true

  // Any layered field from repo.json may also appear here; clone.json wins per §6.5.
  "integrate":                 { "mode": "direct" },
  "review":                    { "gate_command": null },
  "require_remote_on_claim":   true,
  "require_remote_on_review":  true,
  "require_remote_on_close":   true,
  "auto_fetch_on_ready":       true,
  "stale_threshold_seconds":   86400,
  "worktree_dir":              null,
  "protected_main":            true
}
```

`stealth` and `tasks_dir` are clone.json's own — they have no analogue in `repo.json` or `project.json`, since they describe the on-disk checkout's relationship to git itself. All other layered fields are repo-scope-owned with optional per-clone override.

A tracker-scope field (`version`, `id_length`, `min_bl_version`, `plugins`) appearing in `clone.json` aborts the read with a "tracker-scope field in clone.json" diagnostic (§6.5).

### 6.5 Field table and precedence — "specific wins"

| field | tracker.json | project.json | repo.json | clone.json |
|---|:-:|:-:|:-:|:-:|
| `state_url` | **owns** | — | — | — |
| `state_branch` | **owns** | — | — | — |
| `version` | — | **owns** | rejected | rejected |
| `id_length` | — | **owns** | rejected | rejected |
| `min_bl_version` | — | **owns** | rejected | rejected |
| `plugins` | — | **owns** | rejected | rejected |
| `integrate.mode` | — | optional default | **owns; default `direct`** | optional override |
| `review.gate_command` | — | optional default | **owns; default `null`** | optional override |
| `require_remote_on_claim` | — | optional default | **owns; default `true`** | optional override |
| `require_remote_on_review` | — | optional default | **owns; default `true`** | optional override |
| `require_remote_on_close` | — | optional default | **owns; default `true`** | optional override |
| `auto_fetch_on_ready` | — | — | **owns; default `true`** | optional override |
| `stale_threshold_seconds` | — | — | **owns; default `86400`** | optional override |
| `worktree_dir` | — | — | **owns; default `null`** | optional override |
| `protected_main` | — | — | **owns; default `true`** | optional override |
| `stealth` | — | — | — | **owns; default `false`** |
| `tasks_dir` | — | — | — | **owns; default `null`** |

Precedence for layered fields:

```
effective = clone.json field     if present
         ?? repo.json field      if present
         ?? project.json field   if present
         ?? built-in default
```

Read once at the start of every `bl` invocation; cached for that invocation.

Tracker-scope fields (`version`, `id_length`, `min_bl_version`, `plugins`) **abort the read** if they appear in `repo.json` or `clone.json`. Failing on read is louder than ignoring; ignoring would silently drop the field on the next round-trip. The field's owner is unambiguous.

For the per-task `target_branch` override (lives on the task file, not on any config file): the resolution chain is `task.target_branch ?? HEAD@root` (no repo-level default — see §6.7).

### 6.6 Renamed fields

Two fields are renamed as part of the XDG pivot. Both renames landed in bl-dfd1 and carry forward unchanged here.

- **`delivery.mode` → `integrate.mode`**, with values `local-squash` → `direct` and `deferred` → `forge-pr`. The new field name reads what `bl review` is doing (integrating the work branch into the integration branch). Conformance gated by §14 test 16.
- **`review.pre_check` → `review.gate_command`**. The new name says what the field *does* (it gates the review transition) and that the value is a shell command. Conformance gated by §14 test 17.

`bl migrate` (§11) translates the old names to the new on read. SPEC-forge-gated-delivery.md §5 and §7 must be updated to the new names; that cross-document update lands with the implementation balls.

### 6.7 Removed: repo-level `target_branch`

The pre-XDG SPEC carried a repo-level `target_branch` field. bl-dfd1 removed it on the grounds that the smallest unit that legitimately expresses the git-flow choice is the task, and the right repo-level default is `HEAD@root` ("the branch the clone root has checked out"). This revision preserves that decision. The resolution chain remains:

```
effective_target_branch = task.target_branch ?? HEAD@root
```

A user accustomed to a repo-level default sets it by checking out the target branch at the repo root before `bl claim`. This makes the choice visible to git, which is more discoverable than a json field.

### 6.8 Solo case

On a solo repo (no redirect), `tracker.json` is absent; `repo.json` + `project.json` + tasks + plugins all coexist on the same `balls/tasks` branch at distinct paths under `.balls/`. The three-file ownership split holds via "which file the field lives in," not "which branch."

### 6.9 Forward-compat for unknown fields

`project.json`, `repo.json`, and `clone.json` all retain the lenient-unknown-fields invariant (bl-1b07: symmetric unknown=round-trip across serde seams). A future revision adding a field to any of the three config files is observed-and-preserved by older `bl`. The exception is `tracker.json`: §6.1 keeps that schema strict.

## 7. Materialization

`Store::discover` is the entry point. Idempotent; runs on every `bl` invocation. Steps:

1. **Resolve the clone path.** Compute `<nested-clone-path>` from the clone's absolute git-dir path (or `--tasks-dir` / PWD in stealth). Strip the leading `/`.
2. **Read clone.json if present.** Stat `~/.config/balls/<nested-clone-path>/clone.json`. If it carries `stealth: true`, take `tasks_dir` as the task store and skip steps 3–6 entirely. Otherwise, hold its layered-field overrides for step 7.
3. **Derive `<enc-origin>`.** Read `origin.url` from the clone's `.git/config`, canonicalize, percent-encode (§4). If `origin` is missing in non-stealth mode, abort with the "no origin configured" diagnostic.
4. **Fetch the repo's own `balls/tasks`.** Single-branch fetch into `~/.local/state/balls/trackers/<enc-origin>/<enc-balls-tasks>/`. First contact clones; subsequent invocations fetch and fast-forward. If the tracker is unreachable, SPEC-tracker-state §9 applies (hard-fail explicit, soft-fail warm).
5. **Detect redirect.** Stat `.balls/tracker.json` in that checkout. If absent, the active tracker IS the repo's own checkout; skip step 6. If present, parse it (`state_url` required, `state_branch` optional, no other fields per §6.1).
6. **Fetch the federated tracker.** Canonicalize and percent-encode the redirect's `state_url`; percent-encode `state_branch` (default `balls/tasks`). Clone or fetch into `~/.local/state/balls/trackers/<enc-state-url>/<enc-state-branch>/`. If that checkout's `.balls/tracker.json` exists, abort with "chained redirect detected" — single-hop only.
7. **Ensure the per-clone tree.** `mkdir -p` for `~/.local/state/balls/{worktrees,claims,locks,plugins-auth}/<nested-clone-path>/` as needed. Plain `mkdir -p`; safe under concurrent invocations.
8. **Load and validate config.** Parse `project.json` (from the active tracker checkout) and `repo.json` (from the code repo's own checkout). Apply the precedence rule (§6.5) layering clone.json's overrides on top. Tracker-scope fields appearing in `repo.json` or `clone.json` abort the read per §6.5.

There is no derived-key step, no `mkdir` for `<origin-key>` or `<path-hash>` subtrees, no view-symlink construction (the old `active/` and `own/` symlink trees go away). The paths themselves are hand-readable.

Reachability rules (SPEC-tracker-state §9) and merge cleanliness (SPEC-tracker-state §11) carry forward unchanged.

The whole layout is regenerable from `(origin URL, clone path)`. Delete the relevant `trackers/`, `worktrees/`, `claims/`, `locks/`, `plugins-auth/` subtrees and the next `bl` invocation rebuilds them. The only unrecoverable loss is state-branch commits never pushed to the tracker — the ordinary exposure of any unpushed git work.

## 8. Worktree relocation

Task worktrees live at `~/.local/state/balls/worktrees/<nested-clone-path>/<bl-id>/`. `bl claim` invokes `git worktree add` with an absolute off-tree path; the clone's `.git` tracks the worktree as it would any other.

One accepted ergonomic cost: a worktree is no longer colocated under the clone's working tree, so `ls .balls-worktrees/` does not enumerate active claims. The replacement read surface is `bl list` (which already enumerates claims by status) and `ls ~/.local/state/balls/worktrees/<nested-clone-path>/`. The trade was named in the parent epic and accepted.

A moved clone breaks the per-clone state binding: the `<nested-clone-path>` changes, so the old `worktrees/`, `claims/`, `locks/`, and `plugins-auth/` subtrees are orphaned at the old path. Re-priming at the new path re-materializes the binding; the orphaned subtrees at the old path are inert and may be removed by hand.

## 9. Code/state split

SPEC-tracker-state §10 carries forward unmodified. `bl review` squashes the work branch onto the *repo's own* integration branch on the *repo's own* `origin`; the `[bl-xxxx]` delivery tag lands there. Only the state-branch transition reaches the tracker.

This relocation does not touch code delivery. `repo` (the code repo a task belongs to) and `delivered_repo` (where the delivery commit landed) remain on the task file. The constraint that `bl show` in repo C can resolve a task delivered in repo B only if it can reach B is unchanged.

The pre-XDG layout already enforced this split; moving the clone's runtime state into XDG dirs strengthens the separation by removing the last in-repo balls artifact — the `.balls/config.json` file — that gave the impression of a hybrid model. Post-XDG, there is no shared file on the code branch, so no read can mistakenly conflate code-branch state with tracker state.

## 10. Hand-operability

A human with `git`, `jq`, `mkdir`, and `jq -Rr @uri` (a stdlib percent-encoder) can stand up a clone, join a tracker, read state, and detach. The full clone-and-join sequence:

```sh
# Inputs: ORIGIN=<origin URL of the code repo>; clone-path = $(pwd) at the clone root.

# 1. Canonicalize and percent-encode the origin URL into one path component:
canonical=$(echo "$ORIGIN" | sed -E 's#^[a-z]+://##; s#^[^@]+@##; s#\.git$##; s#/$##' | tr 'A-Z' 'a-z')
enc_origin=$(printf '%s' "$canonical" | jq -Rr @uri)
enc_branch=$(printf 'balls/tasks' | jq -Rr @uri)  # = balls%2Ftasks

# 2. Compute the nested clone path (strip leading slash):
nested=$(pwd | sed 's|^/||')

# 3. Ensure the per-clone tree and the tracker checkout dir:
mkdir -p ~/.local/state/balls/trackers/$enc_origin/$enc_branch
mkdir -p ~/.local/state/balls/worktrees/$nested
mkdir -p ~/.local/state/balls/{claims,locks,plugins-auth}/$nested

# 4. Clone the repo's own balls/tasks (the bootstrap branch):
git clone --single-branch --branch balls/tasks "$ORIGIN" \
    ~/.local/state/balls/trackers/$enc_origin/$enc_branch

# 5a. Solo case (no .balls/tracker.json): active = own. Done.
# 5b. Federated case: read the redirect and fetch the tracker too.
tj=~/.local/state/balls/trackers/$enc_origin/$enc_branch/.balls/tracker.json
if [ -e "$tj" ]; then
    state_url=$(jq -r '.state_url' "$tj")
    state_branch=$(jq -r '.state_branch // "balls/tasks"' "$tj")
    canonical_t=$(echo "$state_url" | sed -E 's#^[a-z]+://##; s#^[^@]+@##; s#\.git$##; s#/$##' | tr 'A-Z' 'a-z')
    enc_state_url=$(printf '%s' "$canonical_t" | jq -Rr @uri)
    enc_state_branch=$(printf '%s' "$state_branch" | jq -Rr @uri)
    git clone --single-branch --branch "$state_branch" "$state_url" \
        ~/.local/state/balls/trackers/$enc_state_url/$enc_state_branch
fi

# 6. Read state with stock git + jq:
jq . ~/.local/state/balls/trackers/$enc_origin/$enc_branch/.balls/tasks/bl-abcd.json   # solo
# (in federated: read tasks from the tracker checkout at $enc_state_url/$enc_state_branch)
git -C ~/.local/state/balls/trackers/$enc_origin/$enc_branch log balls/tasks
```

The sequence does not call `sha256sum`. It does not derive an opaque key. Every path printed in `pwd` or in `ls` reads back to its inputs by eye.

Changes that would break this hand-operable property — switching to a non-reversible encoding, reintroducing hashing, adding a level of indirection through a discoverable index file — are breaking changes to this SPEC.

## 11. Migration

A pre-revision clone has, in some combination, the pre-XDG layout (a `.balls/` committed on `main`), the bl-ed32 / bl-dfd1 / bl-fc50 hashed-XDG layout (an `<origin-key>/<path-hash>` tree), or a mix mid-Phase-1. `bl migrate` covers all three.

### 11.1 Pre-XDG → nested XDG

| Pre-XDG artifact | Destination | Action |
|---|---|---|
| `.balls/config.json` on `main` (committed) | `trackers/<enc-origin>/<enc-balls-tasks>/.balls/repo.json` + (if it carried `state_url`) `.balls/tracker.json` | `bl migrate` writes `repo.json` with everything that was repo-owned; writes `tracker.json` only when `state_url` was set; renames `delivery` → `integrate` and `review.pre_check` → `review.gate_command` in flight; drops the field `target_branch` if present (per-task overrides survive on task files). Then `git rm .balls/config.json` on `main`. |
| `.balls/state-repo/` (gitignored runtime checkout of the tracker) | `trackers/<enc-tracker-origin>/<enc-tracker-branch>/` (or `trackers/<enc-own-origin>/<enc-balls-tasks>/` if solo) | `bl migrate` re-fetches into the XDG path and removes the old one. |
| `.balls/tasks`, `.balls/project.json`, `.balls/plugins` symlinks | (none) | Removed; reads go to the checkout directly. |
| `.balls/local/` (claims, locks, plugin auth) | `~/.local/state/balls/{claims,locks,plugins-auth}/<nested-clone-path>/` | `bl migrate` copies contents; old dir removed. |
| `.balls-worktrees/bl-xxxx/` (active task worktrees) | `~/.local/state/balls/worktrees/<nested-clone-path>/bl-xxxx/` | `bl migrate` uses `git worktree move`; refuses if any worktree has uncommitted changes (commit or drop first). |
| `.gitignore` entries (`.balls/state-repo`, `.balls-worktrees`, etc.) | (none) | Removed in the migration commit. |
| `.balls/` directory at the clone root | (none) | Removed in the migration commit. |
| Pre-XDG `.balls/project.json` on tracker | tracker checkout `.balls/project.json` | Stays at the same path on the branch; any field whose new primary owner is `repo.json` is left intact (it becomes a project-wide default — repo.json wins if it sets one too). Field renames applied. |

The migration commit on `main` carries the message `balls: migrate to XDG layout [bl-xxxx]` and is the final balls-attributed commit on `main` for the lifetime of the clone.

### 11.2 Hashed-XDG → nested XDG (the bl-fc50 → bl-e9c2 delta)

Larger than the bl-ed32 → bl-dfd1 split because the on-disk paths change shape, not just the file partitioning on the branch. Branch contents (`.balls/tasks/`, `.balls/repo.json`, `.balls/project.json`, `.balls/plugins/`, `.balls/tracker.json`) are unchanged from bl-fc50; only the *checkout paths* and the per-clone state dirs move.

| bl-fc50 artifact | Destination | Action |
|---|---|---|
| `~/.local/state/balls/<origin-key>/state-repos/own/` (own checkout) | `~/.local/state/balls/trackers/<enc-origin>/<enc-balls-tasks>/` | `bl migrate` re-fetches into the nested path. The old checkout is `rm -rf`'d after the new one is verified. (Re-fetch over `git worktree move` because the new path is governed by an encoded URL component, not a literal rename; rebuilding from `origin` is the cleaner contract.) |
| `~/.local/state/balls/<origin-key>/state-repos/<active-key>/` (federated checkout, if redirected) | `~/.local/state/balls/trackers/<enc-state-url>/<enc-state-branch>/` | Same: re-fetch into the nested path. |
| `~/.local/state/balls/<origin-key>/<path-hash>/worktrees/bl-xxxx/` | `~/.local/state/balls/worktrees/<nested-clone-path>/bl-xxxx/` | `git worktree move` per worktree. Refuses if uncommitted changes exist. |
| `~/.local/state/balls/<origin-key>/<path-hash>/{claims,locks,plugins-auth}/` | `~/.local/state/balls/{claims,locks,plugins-auth}/<nested-clone-path>/` | Copy contents; old dirs removed. |
| `~/.config/balls/<origin-key>/active/` and `own/` symlink trees | (none) | Removed wholesale. The new layout reads from the tracker checkout paths directly; no view symlinks exist. |
| `~/.config/balls/<origin-key>/` (the `<origin-key>` parent itself, after the symlink trees are gone) | (none) | Removed. |

No commit on `main`, no commit on the branch. The bl-fc50 → bl-e9c2 migration is entirely a per-host file move.

`clone.json` is **not created** by the migration. A bl-fc50 clone had no per-on-disk-checkout overrides (the layer didn't exist as a separate file — it was implicit in the directory layout). Post-migration the clone falls through to `repo.json` defaults for every layered field, identical to its bl-fc50 behavior. A user who later needs an override creates `~/.config/balls/<nested-clone-path>/clone.json` by hand or via `bl init --stealth` for stealth conversion.

### 11.3 General properties

`bl migrate` is idempotent: re-running on an already-migrated clone is a no-op (it observes that no `.balls/` exists at the clone root, no `<origin-key>` subtree exists under `~/.local/state/balls/`, and nothing on `main` needs to be removed). It refuses to run if the clone has uncommitted changes on `main` or in any task worktree.

Migration is one-way. There is no `bl unmigrate`. A clone that wants a pre-revision layout back reverts the migration commit by hand and reconstructs the old paths manually; this is a recovery path, not a supported workflow. §13 rules out a `--classic` toggle.

## 12. Backwards-compatibility audit

Carries over the row-by-row analysis of SPEC-tracker-state §12 and prior revisions' §12, updated for the nested-path layout:

| Scenario | Behavior | Risk | Mitigation |
|---|---|---|---|
| New `bl`, repo with no balls history | `bl init` creates `balls/tasks` on `origin` with a default `repo.json` and a default `project.json`, no `tracker.json`; XDG dirs materialize on first `discover`. No commit on `main`. | None | Phase 0 default; conformance test §14.1. |
| New `bl`, pre-XDG repo with `.balls/config.json` on `main` and `.balls/state-repo/` runtime | Phase 1 (dual-read) reads both layouts and prefers the new one; if the new one is absent, falls back to the old. Phase 2 (`bl migrate`) is opt-in until Phase 3, when `bl prime --migrate` becomes the default. | A clone that runs Phase-1 `bl` for a long period accumulates state in both layouts and stays correct, but the in-repo `.balls/` does not shrink until migration | Phase 1 emits a one-line "legacy layout in use" warning; migration is one-shot and well-defined. |
| New `bl`, bl-fc50 clone (hashed XDG layout) | Phase 1 dual-read: if the `<origin-key>/<path-hash>` subtree exists and the nested-XDG paths do not, read from the hashed paths and warn that nested migration is pending. Phase 2 (`bl migrate`) re-fetches into the nested paths and `git worktree move`s worktrees. | A clone that never migrates pays the dual-read overhead on every operation; the hashed paths and nested paths can drift if `bl migrate` is interrupted | One-shot migration; warning channel; `bl migrate` resumes safely on retry (idempotent). |
| Pre-revision `bl` on a post-nested-XDG clone | Pre-revision `bl` looks for `<origin-key>` under `~/.local/state/balls/` and finds none; reports "not a balls repo" or "missing state-repo" and exits. It cannot misroute because it cannot find a starting point. | Pre-revision agents stop being able to use the clone until they upgrade | Documented; version-advised via `min_bl_version` on `project.json`. Same documented-not-engineered caveat from prior revisions, in the same stronger form (silent misroute is impossible because the binding is broken, not soft-redirected). |
| `bl` on a clone with `tracker.json` carrying an unexpected field | Aborts read with "tracker.json schema: unexpected field" (§6.1). | A hand-edited `tracker.json` halts every `bl` operation until the field is removed | Strict schema is intended; the file is small enough that this is correct strictness. |
| Tracker-scope field (e.g. `min_bl_version`) appearing in `repo.json` or `clone.json` | Aborts read with "tracker-scope field in <file>" diagnostic (§6.5). | An older `bl` migration that wrote the field to the wrong file blocks new `bl`. | `bl migrate` handles the recovery; the diagnostic names the field and offers the fix. |
| Two clones of the same origin on the same machine, concurrent operation | Share the tracker checkout at `trackers/<enc-origin>/<enc-balls-tasks>/` (so one fetch refreshes both) but isolate per-clone state under disjoint `<nested-clone-path>` subdirs. | None | Conformance test §14.3. |
| Shared `$HOME` across machines (NFS, sync) | Same `<enc-origin>` and same `<nested-clone-path>` resolve to the same XDG paths. State-repo checkouts and per-clone trees would be shared across hosts. | NFS-style `$HOME` is rare among balls users; concurrent fetches into the shared checkout could race; per-clone trees would not be isolated per-host. | Treat the same as two clones on one machine; rely on git's own `index.lock` to serialize fetches. A user wanting per-host isolation chooses host-specific clone paths. |
| `.balls/tracker.json` on a code repo's own branch pointing to a tracker whose own `balls/tasks` also carries `.balls/tracker.json` (chained redirect) | `bl prime` aborts on the federated tracker fetch with "chained redirect detected" (§5). | None — defense-in-depth check that the schema already disallows | Conformance test §14.5. |
| URL alias drift: same repo cloned via SSH and HTTPS in two clones | Two clones produce two `<enc-origin>` values (different canonical strings) and so two tracker checkouts. Their per-clone trees do not share state. | A user who genuinely intended the two clones to share tracker state will see them diverge | Recommend `git remote set-url` to normalize the URL; documented in §13 non-goals. The previous SPEC's hashed `<origin-key>` had the same property — this revision does not regress here. |

## 13. Non-goals

The non-goals from SPEC-tracker-state §14 carry forward without change:
- A daemon, server, or sync service on the tracker.
- A custom merge engine; conflict resolution remains the field-wise resolver on the text-mergeable schema.
- N-way config reconciliation.
- Cross-tracker task movement.
- Securing the tracker; access control remains git's.
- Per-clone overrides of tracker-scope fields (`version`, `id_length`, `min_bl_version`, `plugins`). The shadowing rule §6.5 covers layered fields only; tracker-scope fields are not layered.

Added in earlier revisions / continued here:

- **No in-repo `.balls/` fallback.** The migration is one-way (idempotent forward, manual reverse). There is no `--classic` toggle, no `BALLS_CLASSIC_LAYOUT` env var, no per-clone "use the old layout" config. The nested XDG layout is the layout.
- **No `$XDG_*` collapse to `~/.balls/`.** Users with non-standard `$XDG_CONFIG_HOME` / `$XDG_STATE_HOME` / `$XDG_CACHE_HOME` are honored.
- **No per-clone overrides of canonicalization.** The URL canonicalization (lowercase scheme/host, strip `.git`, strip trailing slash, strip userinfo) and the percent-encoding rule are constants in the binary. Users who clone the same repo via two different URLs (HTTPS vs SSH) get two `<enc-origin>` values; the recommended fix is `git remote set-url` to normalize, not a URL-alias layer in balls.
- **No `workspace.json` resurrection.** Retired by bl-dfd1; not coming back.
- **No repo-level `target_branch`.** §6.7. The fallback chain is `task.target_branch ?? HEAD@root`; users wanting a repo-wide default check out the branch at the repo root.
- **No project-side overriding of tracker-scope fields from `repo.json` or `clone.json`.** §6.5 makes this explicit.

Added in bl-e9c2:

- **No hashing.** Every path is a transparent assembly of percent-encoded URL components and the literal clone filesystem path. A future change that reintroduces a hash for "compactness" or "URL-safety beyond percent-encoding" is a breaking change to this SPEC.
- **No view symlinks.** The previous `active/` and `own/` symlink trees are not coming back. Reads go to the tracker checkout paths directly; the paths are short enough and discoverable enough that a synthetic view layer is unnecessary.
- **No four-or-more-file config split.** A future field whose primary owner is the tracker goes on `project.json`; whose primary owner is the repo goes on `repo.json`; whose primary owner is the on-disk checkout goes on `clone.json`. The redirect pointer's only home is `tracker.json`. The four-file shape is the layout.

## 14. Conformance tests

Numbered list of behaviors the test suite must gate. Each test must (a) exist before its corresponding implementation ball lands and (b) fail for the right reason against pre-implementation code. The pattern is the same as SPEC-tracker-state §16: new-side assertions, not pinned old-binary fixtures.

1. **XDG paths from a fresh `bl init`.** A clone initialized with `bl init` has no `.balls/` directory at the clone root; on first `bl prime` the `~/.local/state/balls/trackers/<enc-origin>/<enc-balls-tasks>/` and `~/.local/state/balls/{worktrees,claims,locks,plugins-auth}/<nested-clone-path>/` paths exist and are correctly populated. `repo.json` exists on the own branch at `.balls/repo.json`; `project.json` exists on the own branch at `.balls/project.json`; `.balls/tracker.json` does **not** exist on the own branch (solo case by default). No commit on `main` is created by `bl init` or `bl prime`.
2. **Origin URL encoding.** Given a fixed list of input URLs (HTTPS, SSH, with/without `.git`, with/without trailing slash, mixed case), the canonicalization + percent-encoding produces the documented `<enc-origin>` values. Frozen golden vectors so a future implementation cannot drift. The encoding is round-trippable: decode + reverse-canonicalize is not required, but two URLs that should collide (e.g. `Foo.git` and `foo`) must encode to the same value.
3. **Per-clone isolation by nested path.** Two clones of the same origin into different filesystem directories share `trackers/<enc-origin>/<enc-balls-tasks>/` (same `<enc-origin>`) but have distinct `<nested-clone-path>` subdirs under `worktrees/`, `claims/`, `locks/`, and `plugins-auth/`. Concurrent `bl claim` of two different tasks from the two clones writes to disjoint claim files and does not deadlock or corrupt either clone's locks.
4. **Bootstrap branch is constant.** `bl` does not consult any file at any path to determine the bootstrap branch name. Mutating any config file to override the branch name has no effect — the binary fetches `origin balls/tasks` regardless. (The file *contents* may carry a `state_branch` for the *redirect* target; the bootstrap branch name itself is not configurable.)
5. **Redirect — single hop only.** A `.balls/tracker.json` on a repo's own branch with `state_url` set resolves to one federated tracker checkout. If that tracker's `balls/tasks` carries a `.balls/tracker.json` (synthetic test fixture), `bl prime` aborts with the "chained redirect detected" error.
6. **`tracker.json` is pointer-only.** A `tracker.json` carrying any field other than `state_url` and `state_branch` aborts `bl prime` with the "tracker.json schema: unexpected field" diagnostic (§6.1). `tracker.json` with neither `state_url` nor `state_branch` is also rejected.
7. **`tracker.json` absent on a solo repo.** A repo whose own `balls/tasks` carries no `.balls/tracker.json` resolves to the own checkout as active; `repo.json` and `project.json` coexist on the same branch at distinct `.balls/` paths; `bl list`, `bl ready`, and `bl prime` all behave identically to a repo where the active checkout differs from the own checkout. The presence-or-absence of `.balls/tracker.json` is the sole structural difference.
8. **Three-way precedence on a layered field.** A field set in `clone.json`, `repo.json`, and `project.json` (e.g. `integrate.mode = forge-pr` in clone, `direct` in repo, `forge-pr` in project) reads as the `clone.json` value. With only `repo.json` set, reads return repo's value. With only `project.json` set, reads return project's value. With none set, reads return the built-in default.
9. **Tracker-scope fields rejected outside `project.json`.** A `repo.json` or `clone.json` containing `min_bl_version`, `id_length`, `version`, or `plugins` aborts the read with the documented diagnostic (§6.5). Test exercises one field of each type in each of the two wrong locations, asserting the read errors rather than silently dropping or applying the value.
10. **Disjoint primary owners.** Programmatically: the union of `tracker.json`'s schema fields, the tracker-scope fields of `project.json`, `repo.json`'s primary-owned fields, and `clone.json`'s own fields (`stealth`, `tasks_dir`) has no field appearing in more than one set. The layered-field overlap (project default, repo primary, clone override) is enumerated explicitly and excluded from this check.
11. **Materialization is idempotent.** Running `bl prime` twice produces a `~/.local/state/balls/` tree with identical inode-level state; no churn, no rebuilds, no error.
12. **State-repo regenerability.** Deleting `~/.local/state/balls/trackers/` and running `bl prime` rebuilds the checkouts from the trackers; only state-branch commits never pushed are lost.
13. **Worktrees relocated.** `bl claim bl-xxxx` creates a worktree at `~/.local/state/balls/worktrees/<nested-clone-path>/bl-xxxx/`. The clone's working tree contains no `.balls-worktrees/` directory and `.gitignore` contains no balls-related entries.
14. **Moved clone re-binds on prime.** A clone whose `<nested-clone-path>` no longer matches recorded per-clone state re-materializes its binding on `bl prime`; the orphaned per-clone state at the old nested path is inert (removable by hand), with no data loss.
15. **Stealth via `clone.json`.** `bl init --stealth --tasks-dir /tmp/store` writes `~/.config/balls/tmp/store/clone.json` carrying `stealth: true` and `tasks_dir: /tmp/store`. Subsequent `bl` invocations from that directory read the tasks store from `/tmp/store` and do **not** fetch any tracker, do **not** create any `trackers/<enc-origin>/...` subtree, do **not** require `origin` to be configured. The same operations against a stealth clone work end-to-end without git remote access.
16. **`integrate.mode` rename gate.** `repo.json` carrying `integrate: { mode: direct }` exercises the local-squash code path; `integrate: { mode: forge-pr }` exercises the forge-deferred code path. The old field name `delivery` is rejected on a freshly-written `repo.json`; the read either rejects, or — under Phase 1 dual-read — applies the rename in memory and warns.
17. **`review.gate_command` rename gate.** `repo.json` carrying `review: { gate_command: "<cmd>" }` exercises the gate-command code path. The old field name `pre_check` is handled identically to the `delivery` case.
18. **No repo-level `target_branch`.** `repo.json` is rejected if it contains a top-level `target_branch` field. The resolution chain `task.target_branch ?? HEAD@root` is the only one exercised by `bl review`.
19. **No on-`main` writes (steady state).** Across a full lifecycle (`bl init`, `bl create`, `bl claim`, `bl review`, `bl close`) on a post-migration clone, `git log main` gains exactly the `[bl-xxxx]` delivery commit from `bl review` — and nothing else. No `balls: *` commits.
20. **Hand-operable join sequence.** The §10 shell sequence, executed with stock `git`, `ln`, `jq`, and `jq -Rr @uri`, produces a layout that subsequent `bl` invocations accept and operate on without further setup. The script covers both the solo branch (no `.balls/tracker.json`) and the federated branch (with `.balls/tracker.json`).
21. **Migration is one-shot — pre-XDG path.** Running `bl migrate` on a pre-XDG clone produces (a) the documented nested XDG tree, (b) one commit on `main` removing the in-repo `.balls/` and `.gitignore` entries, and (c) no further state in the clone's working tree. Re-running `bl migrate` is a no-op.
22. **Migration is one-shot — hashed-XDG path.** Running `bl migrate` on a bl-fc50 clone re-fetches the tracker checkouts into nested paths, moves worktrees with `git worktree move`, copies claims/locks/plugins-auth into the nested layout, and removes the `<origin-key>/<path-hash>` tree wholesale. No commit on `main`, no commit on the state branch. Re-running `bl migrate` is a no-op.
23. **Pre-XDG fallback never reappears (caveat asserted new-side).** New `bl` on a post-migration clone never creates `.balls/` in the clone's working tree, never reads from `.balls/config.json` on `main`, never reads or writes `.balls/workspace.json` on the orphan branch, never derives `<origin-key>` or `<path-hash>`, and never writes to `.balls-worktrees/` — the exact behaviors a pre-revision binary would exhibit. Asserted on the new side rather than as an old-binary fixture: per SPEC-tracker-state §16.13, an old binary's behavior is immutable and a pinned-old fixture passes against pre-spec code and so cannot fail-then-pass across the refactor as the gating pattern requires.
24. **`min_bl_version` advisory.** On a clone whose `project.json` carries `min_bl_version > current`, `bl prime` prints a one-line upgrade warning and continues. The warning lives on the project file, which a sufficiently old `bl` cannot read; this test asserts the warning emits for new `bl` running against a newer-`min_bl_version` project.

No phase ball under bl-f021 lands until its corresponding conformance tests exist and fail for the right reason against pre-implementation code.

## 15. Revision history

**bl-ed32 (commit aa26cc0, 2026-05-22)** — initial draft. Two files on the repo's own branch: `workspace.json` carrying both the redirect pointer and the per-code-repo config; `project.json` carrying the per-project config. `delivery.mode = local-squash | deferred`. `review.pre_check`. Repo-level `target_branch`. Filename: `SPEC-workspace-layout.md`. Hashed two-layer identity (`<origin-key>` from canonical origin URL; `<path-hash>` from absolute git-dir path).

**bl-dfd1 (commit 500b657, 2026-05-23)** — three files. `workspace.json` split into `tracker.json` (pointer-only) + `repo.json` (per-code-repo). Field renames: `delivery` → `integrate`, `review.pre_check` → `review.gate_command`. Repo-level `target_branch` removed (resolution chain becomes `task.target_branch ?? HEAD@root`). Hashed two-layer identity preserved. Filename unchanged.

**bl-fc50 (commit 6e713d5, 2026-05-23)** — terminology sweep. "Workspace" → "clone" for the per-on-disk-checkout concept; "workspace-owned" config (the `repo.json` layer) becomes "repo-owned". SPEC file renamed from `SPEC-workspace-layout.md` to `SPEC-clone-layout.md`. Hashed two-layer identity preserved. No structural or schema change.

**bl-e9c2 (this revision, 2026-05-25)** — structural simplification. The hashed two-layer identity is dropped entirely. Identity now comes from three natural names used directly: the canonical origin URL (percent-encoded into one path component), the orphan branch name (percent-encoded into one path component), and the clone's absolute filesystem path (nested literally, leading slash dropped). The on-disk tree is reorganized accordingly: `trackers/<enc-origin>/<enc-balls-tasks>/` for state-repo checkouts; `worktrees/<nested-clone-path>/<bl-id>/`, `claims/<nested-clone-path>/`, `locks/<nested-clone-path>/`, `plugins-auth/<nested-clone-path>/` for per-clone state. The `active/` and `own/` view-symlink trees are retired (the tracker-checkout paths are now hand-readable directly). A fourth config file is added — `clone.json` (per-on-disk-checkout, uncommitted, under `~/.config/balls/<nested-clone-path>/`) — completing the three-layer shadowing rule `clone.json ?? repo.json ?? project.json ?? built-in default` for layered fields. Stealth/no-git mode is absorbed into `clone.json` (a `stealth: true` flag and a `tasks_dir` path), retiring the previous revisions' "synthetic origin-key" special case. The three open questions from bl-fc50's original three-edit scope — §4 vs §10 lowercase scope, unnamed `<path-hash>` breadcrumb, stealth/no-git under XDG — each pointed at the hashed identity doing work the SPEC didn't need it to do; dropping the hashing collapses all three to "doesn't apply anymore." The four implementation phases (bl-77cb, bl-717e, bl-05e5, bl-a84c) keep their identities; bl-77cb's slice gets re-planned against this rewritten SPEC and the f60262d commit (vendored SHA-256 + clone-identity hashing) is reverted as part of that re-planned slice. No code changes in this ball.
