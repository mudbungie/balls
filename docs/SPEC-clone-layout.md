# SPEC: Clone Layout — XDG dirs, Orphan-Branch Bootstrap, Three-File Config

Status: draft. This file is the third revision of the on-disk layout SPEC. The first revision (bl-ed32, commit aa26cc0) shipped a two-file design (`workspace.json` + `project.json`) where `workspace.json` did double duty as the redirect pointer *and* the per-code-repo config; the second revision (bl-dfd1, commit 500b657) split those into three files named for their layer's scope. This revision (bl-fc50) is a pure terminology sweep — "workspace" → "clone" for the per-on-disk-checkout concept, "workspace-owned" → "repo-owned" for the `repo.json` layer — with no structural change. The SPEC file is renamed from `SPEC-workspace-layout.md` to `SPEC-clone-layout.md`; the bl-dfd1 text is preserved under the old filename in `git show 500b657:docs/SPEC-workspace-layout.md`. See §15 for the revision rationale. The motivation (§1), principles (§2), worktree relocation (§8), code/state split (§9), and non-goals (§13) are unchanged in substance from bl-dfd1.

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

## 2. Principles

The invariants from SPEC-tracker-state §2 hold without exception. Restated for cross-reference and to bind this SPEC's scope to them:

1. **The default is the model.** A repo with no redirect resolves to its own origin's `balls/tasks` branch and behaves bit-identically to the federated case where clone and tracker happen to coincide. There is no "off" and no "standalone mode"; there is the default.
2. **One mechanism, not a mode.** XDG layout is the only layout. There is no `--classic` flag, no in-repo `.balls/` fallback. Once a clone migrates, the in-repo layout ceases to exist on it.
3. **One VCS.** The tracker remains an ordinary git repo; every operation is `git fetch` / `git merge` / `git push` on the state branch. XDG dirs hold checkouts of that repo, nothing more.
4. **The bootstrap fact is a branch name, not a file path.** `bl` is compiled knowing that `origin balls/tasks` is always the first hop on any clone. This SPEC strengthens the earlier "address lives in a file readable before anything else" formulation (SPEC-tracker-state §4 principle 4): the address no longer lives in a file at all. A clone cannot rename or relocate this convention; only its *contents* are repo-owned.
5. **Bilateral mobility.** Repo identity is the origin URL. Re-cloning a repo to a different directory or onto a different machine resolves the same origin-key and rebinds to the same on-disk per-repo state automatically. Federation redirects are still set with `bl remaster` and removed with `bl remaster --detach`; both are offline-capable.
6. **Ownership is split across three files, each named for its layer's scope, with one precedence rule.** Three files — `tracker.json` (pointer-only), `repo.json` (per-code-repo), `project.json` (per-project) — partition the configuration surface by *whose scope owns the field*. Most fields have exactly one primary owner, so reading them needs no precedence rule. The small set of fields that legitimately exist at both the repo and project layers (a project may set a default; a repo may override) follow the **specific-wins** rule: `repo.json` beats `project.json` beats built-in default. Project-only fields appearing in `repo.json` are rejected on read (§6).
7. **Hard-fail first contact, soft-fail warm.** First-time materialization against an unreachable explicit tracker aborts loudly. A warm checkout works offline.
8. **Hand-operable.** A human with `git`, `ln`, `jq`, and `sha256sum` can stand up a clone, join a tracker, read state, and detach (§10). The XDG paths are reproducible from the origin URL without consulting any bl-specific state.
9. **Merge cleanliness is unconditional.** Carries forward unchanged from SPEC-tracker-state §11.

One addition specific to this SPEC:

10. **Identity has two layers — repo (origin URL) and clone (path within that repo).** The repo layer is keyed by the canonicalized `origin` URL; every on-disk checkout of the same code repo resolves the same repo identity. The clone layer is keyed by the absolute git-directory path under that repo. Two clones of the same code repo (different directories on disk, possibly different machines via a shared home) share the per-repo parts of the layout — the state-repo checkout cache, the tracker access — and isolate the per-clone parts — claims, locks, worktrees — under a deterministic `path-hash` subkey. The repo layer corresponds to bl-e609's "workspace-owned" scope, just renamed and made explicit.

## 3. Layout

XDG-strict. No `~/.balls/` convenience collapse; the three dirs (config, state, cache) play their canonical roles.

```
~/.config/balls/<origin-key>/
  active/                            # symlinks into whichever state-repo is currently active
    project.json -> ../../../../.local/state/balls/<origin-key>/state-repos/<active-key>/.balls/project.json
    tasks        -> ../../../../.local/state/balls/<origin-key>/state-repos/<active-key>/.balls/tasks
    plugins      -> ../../../../.local/state/balls/<origin-key>/state-repos/<active-key>/.balls/plugins
  own/                               # symlinks into the repo's own origin balls/tasks checkout
    repo.json    -> ../../../../.local/state/balls/<origin-key>/state-repos/own/.balls/repo.json
    tracker.json -> ../../../../.local/state/balls/<origin-key>/state-repos/own/.balls/tracker.json  # only when redirected; absent in the solo case

~/.local/state/balls/<origin-key>/
  state-repos/
    own/                             # checkout of <origin-url> balls/tasks (the repo's own)
      .balls/
        repo.json                    # per-code-repo config (§6)
        tracker.json                 # redirect pointer; OPTIONAL — present only on repos that redirect (§6)
    <active-key>/                    # checkout of the federated tracker, when redirected
      .balls/
        tasks/                       # task files (the project state)
        project.json                 # per-project config (§6)
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

`<origin-key>` and `<path-hash>` are explicit functions of inputs (§4); a human can compute them on the command line. The `active/` subdir is the hand-readable surface: `jq . ~/.config/balls/<origin-key>/active/project.json` works on any clone.

`active/` and `own/` collapse to the same target in the solo case where the repo's own `balls/tasks` *is* the tracker — `<active-key>` equals `own` and `tracker.json` is absent (no redirect to follow). The symlink targets for `active/*` and `own/repo.json` then point into the same checkout, and three files coexist on that branch at disjoint paths: `.balls/repo.json`, `.balls/project.json`, and `.balls/tasks/`. This is by design: the layout's shape does not change between solo and federated, only the resolved targets and the presence of `tracker.json`.

Nothing under the clone's working tree is balls-shaped. No `.balls/`, no `.balls-worktrees/`, no `.gitignore` entries. A fresh `git clone` of the code repo shows zero balls footprint.

## 4. Clone identity

Identity has two layers, keyed by inputs the user already has.

**`<origin-key>`** — derived from the clone's `origin` remote URL. The URL string is canonicalized (lowercased scheme/host, trailing `.git` stripped, no trailing slash, no userinfo), then hashed with SHA-256, hex-encoded, and truncated to the first 16 characters. Example:

```
git@github.com:mudbungie/balls.git
  → canonical: github.com:mudbungie/balls
  → sha256(canonical) | hex | head -c 16 = e9f3a7c41b8d2056   (illustrative)
```

The `<origin-key>` is the *repo layer* of identity: two clones of the same origin — same machine, different directories; or different machines via a shared home — resolve the same `<origin-key>`. Their state-repo checkouts (`state-repos/own/`, `state-repos/<active-key>/`) and active-view symlinks are shared. Tracker access is shared too: one fetch refreshes both clones' view.

**`<path-hash>`** — derived from the absolute filesystem path of the clone's git directory (`git rev-parse --absolute-git-dir`). Same hash function, same truncation. This is the *clone layer* of identity: it isolates per-clone state — claims, locks, plugin auth, worktrees — so two clones of the same origin on the same machine do not collide. A moved clone gets a new `<path-hash>` automatically.

A pre-remote clone (one with no `origin` configured) cannot derive `<origin-key>`. This SPEC does not engineer around that: the bootstrap convention (§5) requires `origin` to exist. A solo project that wants balls before pushing anywhere uses `bl init` to create an initial origin (the offline-bootstrappable case is `bl init` with a local bare repo as origin).

## 5. Bootstrap convention

`bl` is compiled knowing one fact about every clone: **the orphan branch `balls/tasks` on `origin` is always the first hop**. This is not a configuration value, not a discoverable file, not overridable per-clone. It is a constant baked into the binary.

The contents of that branch determine what happens next:

1. **Branch absent.** The repo is uninitialized. `bl init` creates the branch with a default `repo.json` and a default `project.json`, no `tracker.json`, and pushes.
2. **Branch present, `tracker.json` absent.** This branch IS the tracker. Task files (`.balls/tasks/`), per-project config (`.balls/project.json`), plugin config (`.balls/plugins/`), and per-code-repo config (`.balls/repo.json`) all live on the same branch. Solo / non-federated case.
3. **Branch present, `tracker.json` present.** Follow the redirect to the federated tracker. The file is *pointer-only*: it carries exactly `state_url` and/or `state_branch`, nothing else. The tracker's `balls/tasks` branch carries the project's state (tasks, project.json, plugins) and is checked out into `state-repos/<active-key>/`. `repo.json` continues to live on the repo's own branch.

The redirect is **single-hop**. A federated tracker may not itself carry a `tracker.json` on its `balls/tasks` branch — the file's only legal location is on a repo's *own* `balls/tasks` branch. Cycle detection is unnecessary in the single-hop model but `bl` validates it as a defense-in-depth check: if any redirect ever resolves into a chain longer than 1, abort with a "chained redirect detected" error. The depth limit is therefore 1, exactly, not a tunable.

That the bootstrap fact is a branch name has one consequence worth naming: a tracker cannot redirect a clone. The branch name is fixed by the binary; only the redirect *contents* are repo-owned (they sit on the repo's own branch). SPEC-tracker-state §2 principle 4 ("the address is the only bootstrap fact") survives in stronger form — the bootstrap fact is now a constant, not a value.

The replaced formulation — earlier text said "the bootstrap reads `workspace.json` and looks for an optional `state_url` field" — is gone. Presence-or-absence of `tracker.json` *is* the signal; reading it never returns "no redirect" because if it's there it carries a pointer.

## 6. Three-file config — tracker.json, repo.json, project.json

Three files, three owners, named for the layer whose scope they capture.

| file | branch it lives on | scope | when present |
|---|---|---|---|
| `tracker.json` | the repo's own `origin balls/tasks` | redirect pointer | only when this repo redirects |
| `repo.json` | the repo's own `origin balls/tasks` | per-code-repo | always |
| `project.json` | the federated tracker's `balls/tasks` (which IS the repo's own in the solo case) | per-project | always |

The pre-revision file `workspace.json` is split: the redirect pointer becomes `tracker.json` (and now has nothing else in it), and everything else repo-owned becomes `repo.json`. The motivation is structural: the previous file conflated two layers of meaning — *"this branch is/isn't the tracker"* and *"how does this code repo integrate"* — into one schema. Separating them lets `tracker.json`'s presence be the redirect signal at the filesystem level, lets `repo.json` exist on every repo whether or not it redirects, and removes the only field set on `workspace.json` that didn't fit cleanly under "repo identity."

### 6.1 `tracker.json` — pointer-only

Schema:

```json
{
  "state_url": "<url>",
  "state_branch": "<branch-name>"   // optional; defaults to "balls/tasks"
}
```

That is the whole schema. No other fields are accepted. Implementations reject extra fields on read with a "tracker.json schema: unexpected field" error rather than rounding-tripping them, because the file is so small that an unexpected field is almost certainly a stale write from an older binary or a hand-edit gone wrong, and silently round-tripping makes the redirect mechanism look more configurable than it is.

The file's *presence* is the bootstrap signal (§5). If the file is absent on a repo's own branch, the branch is the tracker; the redirect contents are simply not asked for. Writing `tracker.json` with no `state_url` is illegal (it would be a tracker.json that says "redirect to nowhere"); `bl remaster` either writes a real pointer or removes the file.

The field names `state_url` and `state_branch` are preserved from SPEC-tracker-state §5 so that `bl remaster` semantics carry over unchanged. Only the file the redirect lives in has changed.

### 6.2 `repo.json` — per-code-repo config

`repo.json` lives on the repo's own `origin balls/tasks` branch. It is the file that carries everything that varies *per code repo*: how `bl review` integrates work into the integration branch, what command gates review, what protections this code repo has on `main`, where worktrees go, and which remote-round-trip policies are in force.

Schema (field-by-field rationale in §6.4):

```json
{
  "integrate": {
    "mode": "direct"            // "direct" | "forge-pr"
  },
  "review": {
    "gate_command": null         // optional shell command; null disables
  },
  "require_remote_on_claim":  true,
  "require_remote_on_review": true,
  "require_remote_on_close":  true,
  "auto_fetch_on_ready":      true,
  "stale_threshold_seconds":  86400,
  "worktree_dir":             null,
  "protected_main":           true
}
```

All fields are optional; an absent field reads as its default (defaults in §6.4). A repo with no balls-specific configuration ships a zero-keys `repo.json` and gets the defaults.

Two field-name changes from bl-ed32's `workspace.json` schema are baked in here; both are scoped to this SPEC's pivot and are addressed in §6.5 (rename rationale).

### 6.3 `project.json` — per-project config

`project.json` lives on the *tracker's* `balls/tasks` branch (which, in the solo case, is the same branch as `repo.json` and `tracker.json`'s slot, but at a different path). It carries everything that varies *per project*: store schema, id length, minimum-bl-version advisory, plugin configuration, and project-wide defaults for fields whose primary owner is `repo.json`.

Schema:

```json
{
  "version":         "<store schema>",
  "id_length":       4,
  "min_bl_version":  "0.4.0",
  "plugins":         { /* plugin config + participant policy */ },

  // Optional project-wide defaults for repo-owned fields. Repo.json wins per §6.6.
  "integrate": { "mode": "direct" },
  "review":    { "gate_command": null },
  "require_remote_on_claim":  true,
  "require_remote_on_review": true,
  "require_remote_on_close":  true
}
```

Project-only fields (`version`, `id_length`, `min_bl_version`, `plugins`) are the *only* fields whose primary owner is `project.json`. They have no analogue in `repo.json`. Repo-owned fields that may legitimately appear here are explicitly enumerated as **project-wide defaults**; the precedence rule §6.6 governs how they combine with `repo.json`'s values.

### 6.4 Field table

The whole configuration surface, three columns, by primary owner:

| field | tracker.json | repo.json | project.json |
|---|:-:|:-:|:-:|
| `state_url` | **own** | — | — |
| `state_branch` | **own** | — | — |
| `integrate.mode` | — | **owns; default `direct`** | optional default (repo wins) |
| `review.gate_command` | — | **owns; default `null`** | optional default (repo wins) |
| `require_remote_on_claim` | — | **owns; default `true`** | optional default (repo wins) |
| `require_remote_on_review` | — | **owns; default `true`** | optional default (repo wins) |
| `require_remote_on_close` | — | **owns; default `true`** | optional default (repo wins) |
| `auto_fetch_on_ready` | — | **owns; default `true`** | — |
| `stale_threshold_seconds` | — | **owns; default `86400`** | — |
| `worktree_dir` | — | **owns; default `null` (XDG path)** | — |
| `protected_main` | — | **owns; default `true`** | — |
| `version` | — | — | **owns** |
| `id_length` | — | — | **owns** |
| `min_bl_version` | — | — | **owns** |
| `plugins` | — | — | **owns** |

Two design notes about the table:

- **`target_branch` is gone.** Both the previous SPEC's `config.target_branch` and any per-task `target_branch` override remain valid task-schema concepts (see SPEC-forge-gated-delivery §6); the *repo-level default* — what the previous SPEC put on `workspace.json` and what SPEC-forge-gated-delivery §5 documented under `config` — is dropped. Rationale in §6.5.
- **`auto_fetch_on_ready`, `stale_threshold_seconds`, `worktree_dir`, `protected_main` are repo-only.** These are mechanical / local-environment fields with no useful project-wide default; allowing them on `project.json` would dilute the file's meaning ("project policy") without buying anything. If a project later finds it needs to advise a default for one of them, it can be promoted in a future revision.

### 6.5 Renamed fields

Two fields are renamed as part of this revision. The bl-ed32 ↔ bl-dfd1 boundary is the cheap moment to do it — the XDG pivot is already a breaking change to the file layout, so a rename ridealong costs nothing extra.

#### 6.5.1 `delivery` → `integrate`

The previous SPEC (and SPEC-forge-gated-delivery.md §5) carried `delivery: { mode: local-squash | deferred }` on `workspace.json`. The names underspecify: "delivery of what?" doesn't read cold; "deferred to when?" requires knowing the forge-PR mode exists.

Three candidate name sets were considered:

| set | block / values | reads as | objection |
|---|---|---|---|
| A | `integrate: { mode: direct \| forge-pr }` | "integration is *direct* (squash locally now) or *forge-pr* (hand off to a forge)" | none |
| B | `merge: { style: squash \| forge-gated }` | "the merge is *squashed locally* or *gated by the forge*" | `merge` collides with literal `git merge`, which `bl review` runs internally before squash; a reader could plausibly think this configures the merge step rather than the squash step |
| C | `delivery: { mode: squash-local \| pull-request }` | "delivery is *squash locally* or *pull-request*" | keeps the `delivery` umbrella, which was the unclear word; values are clearer but the field name is no better than before |

**Chosen: Set A — `integrate.mode = direct | forge-pr`.** Picks the verb that most directly names what `bl review` is doing (integrating the work branch into the integration branch), and names the mechanism rather than the intent. `direct` and `forge-pr` both stand on their own to a new reader.

The rename propagates through:

- `repo.json` schema (§6.2).
- `project.json` schema if used as a project-wide default (§6.3).
- Task-schema field — the per-task override remains the smallest unit (see §6.6 below); only the *block name* and *values* change.
- [SPEC-forge-gated-delivery.md](SPEC-forge-gated-delivery.md) — its §5 (config schema additions) and §7 (review mechanism) must be updated to the new names. That update is out of scope for this ball (this SPEC owns the field tables; the cross-document rename lands with the implementation balls), but the contract this SPEC publishes uses the new names.
- `bl migrate` (§11) translates the old names to the new on read.

Conformance is gated by §14 test 16.

#### 6.5.2 `review.pre_check` → `review.gate_command`

The previous SPEC carried `review.pre_check` on `workspace.json`. "Pre-check of what?" reads ambiguously: pre-check before review (correct), pre-check of the work branch (also correct), pre-check that gates close (incorrect, but not obviously so).

Three candidates:

| set | name | reads as | objection |
|---|---|---|---|
| A | `review.gate_command` | "the command that gates review" | none |
| B | `review.check_command` | "the command run as a check during review" | "check" is vague — what *kind* of check? |
| C | `integrate.gate` | "the gate before integration" — moves under the renamed integrate block | couples the gate to integration; in `forge-pr` mode the gate runs at `bl review` time and the integrate phase happens later on the forge, so logically the gate belongs to the `bl review` step, not the integrate step |

**Chosen: Set A — `review.gate_command`.** `gate` names what the field *does* (it blocks the lifecycle transition if the command fails); `_command` says explicitly that this is a shell command rather than a list of checks or a config struct.

Conformance is gated by §14 test 17.

### 6.6 Precedence — "specific wins"

For fields with a primary owner in `repo.json` that may also appear as a project-wide default in `project.json`:

```
effective = repo.json field        if present
         ?? project.json field     if present
         ?? built-in default
```

Read once at the start of every `bl` invocation; cached for that invocation. No per-call override.

For fields whose primary owner is `project.json` (`version`, `id_length`, `min_bl_version`, `plugins`):

- If the field appears in `repo.json`, **read fails with a "project-only field in repo.json" diagnostic**. The field's owner is unambiguous; finding it in the wrong file is almost certainly an older `bl` writing under the previous schema, and the migration path (§11) is the supported way to clean it up. Failing on read is louder than ignoring, and ignore would silently drop the field on the next round-trip.

For the per-task `target_branch` override (lives on the task file, not on either config file): the resolution chain is now `task.target_branch ?? HEAD@root` (the middle term `repo.target_branch` from the previous SPEC is dropped — see §6.7).

### 6.7 Removed: repo-level `target_branch`

The previous SPEC and SPEC-forge-gated-delivery §5 put a repo-level `target_branch` field on `workspace.json` to set the default integration branch (e.g. `develop` on a git-flow repo). This revision removes the repo-level field.

The reasoning is workflow-shaped, not platform-shaped:

- The smallest unit that legitimately expresses the git-flow choice is the **task** — a hotfix targets `main`, a feature targets `develop`. The task-level `target_branch` override (already in the task schema, see SPEC-forge-gated-delivery §6) covers this exactly.
- The repo-level default is `HEAD@root` ("the branch the clone root has checked out") — which on a git-flow repo is `develop` if the user checked out `develop`, and `main` otherwise. This is the right default: it lets the user express "integrate work onto this branch" by checking it out at the repo root, the most direct git surface there is.
- Allowing `repo.target_branch` to override `HEAD@root` introduces a state that disagrees with `git branch --show-current` at the repo root. That divergence is exactly the kind of "configuration hides what's actually true" that previous balls have backed away from.

Resolution chain after this revision:

```
effective_target_branch = task.target_branch ?? HEAD@root
```

`HEAD@root` is `git symbolic-ref --short HEAD` evaluated at the clone root (bare hub: the bare directory; non-bare clone: the worktree root). `bl review` reads it once at the start of the review.

A user accustomed to a repo-level default sets it by checking out the target branch at the repo root before `bl claim`. This makes the choice *visible to git*, which is more discoverable than a json field. Existing tasks that already set `task.target_branch` continue to work without change.

The behavioral consequences for SPEC-forge-gated-delivery §5 (which currently declares "in `deferred` mode, null is rejected — PR base must be explicit") flip from a config-level check to a per-task check: in `forge-pr` mode, the PR base is `task.target_branch ?? HEAD@root` at the time `bl review` runs; if neither yields a non-null value, the review aborts asking the user to either check out the target branch or set the per-task override. That cross-document update lands with the implementation balls.

### 6.8 Solo case

On a solo repo (no redirect), `tracker.json` is *absent*; `repo.json` + `project.json` + tasks + plugins all coexist on the same `balls/tasks` branch at distinct paths under `.balls/`. The bl-e609 schema split holds via "which file the field lives in," not "which branch."

### 6.9 Forward-compat for unknown fields

`repo.json` and `project.json` both retain the lenient-unknown-fields invariant (bl-1b07: symmetric unknown=round-trip across serde seams). A future revision adding a field to either file is observed-and-preserved by older `bl`. The exception is `tracker.json`: §6.1 above makes that schema strict, on the grounds that the file is small enough that unknown fields are almost certainly bugs.

## 7. Materialization

`Store::discover` is the entry point. Idempotent; runs on every `bl` invocation. Steps:

1. **Derive `<origin-key>`.** Read `origin.url` from the clone's `.git/config`, canonicalize, hash, truncate (§4). If `origin` is missing, abort with the "no origin configured" diagnostic.
2. **Ensure the XDG tree.** Create `~/.config/balls/<origin-key>/`, `~/.local/state/balls/<origin-key>/`, `~/.cache/balls/<origin-key>/` as needed. Each is a plain `mkdir -p`; safe under concurrent invocations.
3. **Fetch the repo's own `balls/tasks`.** Single-branch fetch into `~/.local/state/balls/<origin-key>/state-repos/own/`. First contact clones; subsequent invocations fetch and fast-forward. If the tracker is unreachable, §9 of SPEC-tracker-state decides (the rule carries unchanged: hard-fail explicit, soft-fail warm).
4. **Detect redirect.** If `state-repos/own/.balls/tracker.json` is absent, set `<active-key> = own` and skip to step 6. If present, parse it; it must carry at least `state_url` (and may carry `state_branch`); any other field aborts the read per §6.1.
5. **Fetch the federated tracker.** Compute `<active-key>` from the redirect's `state_url` (same canonicalize-and-hash). Clone or fetch `state_branch` (default `balls/tasks`) into `state-repos/<active-key>/`. Single-hop only; if the tracker's `balls/tasks` branch contains a `tracker.json`, abort with the "chained redirect detected" error.
6. **Materialize the active view.** Create `~/.config/balls/<origin-key>/active/` and place three symlinks pointing into `state-repos/<active-key>/.balls/`: `tasks`, `project.json`, `plugins`. Create `~/.config/balls/<origin-key>/own/repo.json` (always present, pointing into `state-repos/own/.balls/repo.json`). If `tracker.json` exists on `state-repos/own/.balls/`, also symlink `~/.config/balls/<origin-key>/own/tracker.json` to it; otherwise ensure that symlink is removed. Replace stale symlinks; never error on existing correct ones.
7. **Derive `<path-hash>` and ensure the per-clone tree.** Compute from the clone's git directory (§4). Create `~/.local/state/balls/<origin-key>/<path-hash>/{claims,locks,plugins-auth,worktrees}/` as needed.
8. **Load and validate config.** Parse `repo.json` and `project.json`. Apply the precedence rule (§6.6) to construct the effective config. Project-only fields appearing in `repo.json` abort the read per §6.6.

Reachability rules (SPEC-tracker-state §9) and merge cleanliness (SPEC-tracker-state §11) carry forward unchanged. The state-repo checkouts are the same git repos as before, just relocated.

The whole layout is regenerable from `(origin URL, clone path)`. Delete `~/.local/state/balls/<origin-key>/` and the next `bl` invocation rebuilds it. The only unrecoverable loss is state-branch commits never pushed to the tracker — the ordinary exposure of any unpushed git work.

## 8. Worktree relocation

Task worktrees live at `~/.local/state/balls/<origin-key>/<path-hash>/worktrees/bl-xxxx/`. `bl claim` invokes `git worktree add` with an absolute off-tree path; the clone's `.git` tracks the worktree as it would any other.

One accepted ergonomic cost: a worktree is no longer colocated under the clone's working tree, so `ls .balls-worktrees/` does not enumerate active claims. The replacement read surface is `bl list` (which already enumerates claims by status) and `ls ~/.local/state/balls/<origin-key>/<path-hash>/worktrees/`. The trade was named in the parent epic and accepted.

A moved clone breaks the per-clone state binding: the `<path-hash>` changes, so the old `worktrees/` and `claims/` are orphaned at the old hash. `bl doctor` detects this on read by enumerating sibling `<path-hash>` subdirs under `<origin-key>/` and matching their recorded clone path against the current one. On finding orphaned state, `doctor` reports the old path, the new path, the orphaned task IDs, and offers a `bl repair --rebind-path` command to move them. Read-only by default; `--fix` adds the action. No automatic rebind.

## 9. Code/state split

SPEC-tracker-state §10 carries forward unmodified. `bl review` squashes the work branch onto the *repo's own* integration branch on the *repo's own* `origin`; the `[bl-xxxx]` delivery tag lands there. Only the state-branch transition reaches the tracker.

This relocation does not touch code delivery. `repo` (the code repo a task belongs to) and `delivered_repo` (where the delivery commit landed) remain on the task file. The constraint that `bl show` in repo C can resolve a task delivered in repo B only if it can reach B is unchanged.

The pre-XDG layout already enforced this split; moving the clone's runtime state into XDG dirs strengthens the separation by removing the last in-repo balls artifact — the `.balls/config.json` file — that gave the impression of a hybrid model. Post-XDG, there is no shared file on the code branch, so no read can mistakenly conflate code-branch state with tracker state.

## 10. Hand-operability

The §13 sequences of SPEC-tracker-state are replaced. The full clone-and-join sequence:

```sh
# Inputs: ORIGIN=<origin URL of the code repo>; TRACKER=<federated tracker URL or empty>

# 1. Derive origin-key (same function bl uses):
canonical=$(echo "$ORIGIN" | sed -E 's#^[a-z]+://##; s#^[^@]+@##; s#\.git$##; s#/$##' | tr 'A-Z' 'a-z')
origin_key=$(printf '%s' "$canonical" | sha256sum | head -c 16)

# 2. Ensure XDG dirs:
mkdir -p ~/.config/balls/$origin_key/active ~/.config/balls/$origin_key/own
mkdir -p ~/.local/state/balls/$origin_key/state-repos
mkdir -p ~/.cache/balls/$origin_key

# 3. Clone the repo's own balls/tasks (the bootstrap branch):
git clone --single-branch --branch balls/tasks "$ORIGIN" \
    ~/.local/state/balls/$origin_key/state-repos/own

# 4. Always: own/repo.json points at the repo's own checkout.
ln -sfn ../../../../.local/state/balls/$origin_key/state-repos/own/.balls/repo.json \
    ~/.config/balls/$origin_key/own/repo.json

# 4a. Solo case (no tracker.json on the own checkout): active = own.
if [ ! -e ~/.local/state/balls/$origin_key/state-repos/own/.balls/tracker.json ]; then
    ln -sfn ../../../../.local/state/balls/$origin_key/state-repos/own/.balls/tasks \
        ~/.config/balls/$origin_key/active/tasks
    ln -sfn ../../../../.local/state/balls/$origin_key/state-repos/own/.balls/project.json \
        ~/.config/balls/$origin_key/active/project.json
    ln -sfn ../../../../.local/state/balls/$origin_key/state-repos/own/.balls/plugins \
        ~/.config/balls/$origin_key/active/plugins
fi

# 4b. Federated case: read the redirect, clone the tracker, then symlink.
if [ -e ~/.local/state/balls/$origin_key/state-repos/own/.balls/tracker.json ]; then
    state_url=$(jq -r '.state_url'  ~/.local/state/balls/$origin_key/state-repos/own/.balls/tracker.json)
    state_branch=$(jq -r '.state_branch // "balls/tasks"' \
                     ~/.local/state/balls/$origin_key/state-repos/own/.balls/tracker.json)
    canonical_t=$(echo "$state_url" | sed -E 's#^[a-z]+://##; s#^[^@]+@##; s#\.git$##; s#/$##' | tr 'A-Z' 'a-z')
    active_key=$(printf '%s' "$canonical_t" | sha256sum | head -c 16)
    git clone --single-branch --branch "$state_branch" "$state_url" \
        ~/.local/state/balls/$origin_key/state-repos/$active_key
    ln -sfn ../../../../.local/state/balls/$origin_key/state-repos/own/.balls/tracker.json \
        ~/.config/balls/$origin_key/own/tracker.json
    for f in tasks project.json plugins; do
        ln -sfn ../../../../.local/state/balls/$origin_key/state-repos/$active_key/.balls/$f \
            ~/.config/balls/$origin_key/active/$f
    done
fi

# 5. Per-clone tree:
path_hash=$(git -C <clone-path> rev-parse --absolute-git-dir | sha256sum | head -c 16)
mkdir -p ~/.local/state/balls/$origin_key/$path_hash/{claims,locks,plugins-auth,worktrees}

# 6. Read state with stock git + jq:
jq . ~/.config/balls/$origin_key/active/tasks/bl-abcd.json
git -C ~/.local/state/balls/$origin_key/state-repos/own log balls/tasks
```

These sequences must remain valid for the life of the design. A change that breaks them — the canonicalization, the hash function, the directory shape, the file naming — is a breaking change to this SPEC.

## 11. Migration

A pre-revision clone has, in some combination of two possible histories: the pre-XDG layout (a `.balls/` committed on `main`) or the bl-ed32 layout (XDG dirs but a two-file `workspace.json`/`project.json` design). `bl migrate` covers both.

### 11.1 Pre-XDG → three-file XDG

| Pre-XDG artifact | Destination | Action |
|---|---|---|
| `.balls/config.json` on `main` (committed) | `state-repos/own/.balls/repo.json` + (if it carried `state_url`) `state-repos/own/.balls/tracker.json` | `bl migrate` writes `repo.json` with everything that was repo-owned; writes `tracker.json` only when `state_url` was set on the old config; renames `delivery` → `integrate` and `review.pre_check` → `review.gate_command` in flight; drops the field `target_branch` if present (preserved on per-task records that already carry it). Then `git rm .balls/config.json` on `main`. |
| `.balls/state-repo/` (gitignored runtime checkout of the tracker) | `state-repos/<active-key>/` (or `state-repos/own/` if solo) | `bl migrate` re-fetches into the XDG path and removes the old one. |
| `.balls/tasks`, `.balls/project.json`, `.balls/plugins` symlinks | (none) | Removed; the new view lives under `~/.config/balls/<origin-key>/active/`. |
| `.balls/local/` (claims, locks, plugin auth) | `~/.local/state/balls/<origin-key>/<path-hash>/{claims,locks,plugins-auth}/` | `bl migrate` copies contents; old dir removed. |
| `.balls-worktrees/bl-xxxx/` (active task worktrees) | `~/.local/state/balls/<origin-key>/<path-hash>/worktrees/bl-xxxx/` | `bl migrate` uses `git worktree move`; refuses if any worktree has uncommitted changes (caller is expected to commit or drop first). |
| `.gitignore` entries (`.balls/state-repo`, `.balls-worktrees`, etc.) | (none) | Removed in the migration commit. |
| `.balls/` directory at the clone root | (none) | Removed in the migration commit. |
| Pre-XDG `.balls/project.json` on tracker | `state-repos/<active-key>/.balls/project.json` | Stays at the same path; any field whose new primary owner is `repo.json` is left intact (it becomes a project-wide default — repo.json wins if it sets one too). Field renames applied. |

The migration commit on `main` carries the message `balls: migrate to XDG layout [bl-xxxx]` and is the final balls-attributed commit on `main` for the lifetime of the clone.

### 11.2 bl-ed32 two-file layout → three-file layout

| bl-ed32 artifact | Destination | Action |
|---|---|---|
| `state-repos/own/.balls/workspace.json` carrying `state_url` and other fields | `state-repos/own/.balls/tracker.json` (just `state_url` / `state_branch`) + `state-repos/own/.balls/repo.json` (everything else) | `bl migrate` writes both files in one state-branch commit; `git rm` the old `workspace.json`. |
| `state-repos/own/.balls/workspace.json` carrying no `state_url` (solo) | `state-repos/own/.balls/repo.json` only | No `tracker.json` is written; the old file is removed. |
| `delivery` block on `workspace.json` (or `project.json`) | `integrate` block on `repo.json` (or as a project-wide default on `project.json`) | Field rename: `delivery.mode` → `integrate.mode`; values `local-squash` → `direct`, `deferred` → `forge-pr`. |
| `review.pre_check` field | `review.gate_command` field | Field rename only; the value (a shell command or null) carries over verbatim. |
| `target_branch` at the repo level | (none) | Dropped at read time, with a warning. Per-task `target_branch` overrides remain intact. The user's workflow is to check out the desired branch at the repo root (§6.7). |
| Project-only fields (`min_bl_version`, `id_length`, `version`) appearing in `workspace.json` | Quarantined and reported | `bl migrate` does not silently move them; it reports them as misplaced fields and asks the user to confirm before they're either dropped or merged into `project.json`. (In practice no bl-ed32 binary writes these to `workspace.json`; this catches hand-edits.) |
| Symlink `~/.config/balls/<origin-key>/own/workspace.json` | Replaced by `own/repo.json` (and conditionally `own/tracker.json`) | `bl migrate` rewrites the symlink layout. |

The bl-ed32 → three-file migration touches only the state branch (and the user's XDG dir symlinks). It does not commit to `main`.

### 11.3 General properties

`bl migrate` is idempotent: re-running on an already-migrated clone is a no-op (it observes that no `.balls/` exists at the clone root, no `workspace.json` exists on the state branch, and nothing on `main` needs to be removed). It refuses to run if the clone has uncommitted changes on `main` or in any task worktree — the migration commit is too disruptive to silently merge into in-flight work.

Migration is one-way. There is no `bl unmigrate`. A clone that wants the pre-XDG layout back reverts the migration commit by hand and runs `bl detach`; this is a manual recovery path, not a supported workflow. The non-goal in §13 rules out a `--classic` toggle.

## 12. Backwards-compatibility audit

Carries over the row-by-row analysis of SPEC-tracker-state §12 and bl-ed32's §12, updated for the three-file model:

| Scenario | Behavior | Risk | Mitigation |
|---|---|---|---|
| New `bl`, repo with no balls history | `bl init` creates `balls/tasks` on `origin` with a default `repo.json` and a default `project.json`, no `tracker.json`; XDG dirs materialize on first `discover`. No commit on `main`. | None | Phase 0 default; conformance test §14.1. |
| New `bl`, pre-XDG repo with `.balls/config.json` on `main` and `.balls/state-repo/` runtime | Phase 1 (dual-read) reads both layouts and prefers the new one; if the new one is absent, falls back to the old. Phase 2 (`bl migrate`) is opt-in until Phase 3, when `bl prime --migrate` becomes the default. | A clone that runs Phase-1 `bl` for a long period accumulates state in both layouts and stays correct, but the in-repo `.balls/` does not shrink until migration | Phase 1 emits a one-line "legacy layout in use" warning; migration is one-shot and well-defined. |
| New `bl`, bl-ed32 clone (two-file `workspace.json`+`project.json` on XDG) | Phase 1 dual-read: if `workspace.json` is present on the own branch, read it under the old schema and apply the field renames in memory; warn that migration to three-file is pending. Phase 2 (`bl migrate`) splits the file. | A clone that never migrates accumulates the rename overhead on every read | One-shot migration; warning channel. |
| Pre-revision `bl` on a post-three-file clone | Pre-revision `bl` looks for `.balls/workspace.json` (or `.balls/config.json`) and finds neither. It reports "not a balls repo" or "missing config" and exits. It cannot misroute because it cannot find a starting point. | Pre-revision agents stop being able to use the clone until they upgrade | Documented; version-advised via `min_bl_version` on `project.json`. Accepted: same documented-not-engineered caveat from bl-ed32's §12, in the same stronger form (silent misroute is impossible because the binding is broken, not soft-redirected). |
| `bl` on a clone with `tracker.json` carrying an unexpected field | Aborts read with "tracker.json schema: unexpected field" (§6.1). | A hand-edited `tracker.json` halts every `bl` operation until the field is removed | Strict schema is the intended behavior; the file is small enough that this is correct strictness, not over-strictness. |
| Project-only field (e.g. `min_bl_version`) appearing in `repo.json` | Aborts read with "project-only field in repo.json" diagnostic (§6.6). | An older `bl` migration that wrote the field to the wrong file blocks new `bl`. The migration tool (§11) covers this case. | `bl migrate` handles the recovery; the diagnostic names the field and offers the fix. |
| Two clones of the same origin on the same machine, concurrent operation | Share `<origin-key>` (so share the state-repo cache and tracker access — one fetch refreshes both) but isolate `<path-hash>` (so claims, locks, worktrees do not collide). | None | Conformance test §14.3. |
| Shared `$HOME` across machines (NFS, sync) | Same `<origin-key>` resolves to the same XDG paths. `<path-hash>` is per-host because git directories differ. State-repo checkouts shared across hosts; per-host claims/locks isolated. | NFS-style `$HOME` is rare among balls users but technically supported; concurrent fetches into the shared state-repo could race | Treat the same as two clones on one machine for the per-clone layer; for the shared layer, rely on git's own `index.lock` to serialize fetches. |
| `tracker.json` carrying `state_url` to a tracker whose own `balls/tasks` carries a `tracker.json` (chained redirect) | `bl prime` aborts on the federated tracker fetch with "chained redirect detected" (§5). | None — defense-in-depth check that the schema already disallows | Conformance test §14.5. |

## 13. Non-goals

The non-goals from SPEC-tracker-state §14 carry forward without change:
- A daemon, server, or sync service on the tracker.
- A custom merge engine; conflict resolution remains the field-wise resolver on the text-mergeable schema.
- N-way config reconciliation.
- Cross-tracker task movement.
- Securing the tracker; access control remains git's.
- Per-clone overrides of project-owned fields (`version`, `id_length`, `min_bl_version`, `plugins`). The precedence rule §6.6 covers fields that are *owned by repo and defaulted by project*; it does not introduce overrides for project-only fields.

Added by bl-ed32 / continued here:

- **No in-repo `.balls/` fallback.** The migration is one-way (idempotent forward, manual reverse via `git revert` on `main` plus `bl detach`). There is no `--classic` toggle, no `BALLS_CLASSIC_LAYOUT` env var, no per-clone "use the old layout" config. The XDG layout is the layout.
- **No `$XDG_*` collapse to `~/.balls/`.** Users with non-standard `$XDG_CONFIG_HOME` / `$XDG_STATE_HOME` / `$XDG_CACHE_HOME` are honored. A single `~/.balls/` would be simpler but it is not XDG, and the XDG spec is what the rest of the user's tooling assumes.
- **No per-clone overrides of `<origin-key>` derivation.** The canonicalization and hash function are constants in the binary. Users who clone the same repo via two different URLs (HTTPS vs SSH) get two `<origin-key>`s; the recommended fix is to normalize the remote URL with `git remote set-url`, not to engineer a URL-alias layer into balls.

Added by bl-dfd1 / continued here:

- **No `workspace.json` resurrection.** The pre-revision file name is retired. A future field whose primary owner is the repo goes on `repo.json`; a future field whose primary owner is the project goes on `project.json`; the redirect pointer's only home is `tracker.json`. The three-file shape is the layout, not a stepping stone to a four-file one.
- **No repo-level `target_branch`.** §6.7. The fallback chain is `task.target_branch ?? HEAD@root`; users wanting a repo-wide default check out the branch at the repo root.
- **No project-side overriding of project-only fields from `repo.json`.** §6.6 makes this explicit.

## 14. Conformance tests

Numbered list of behaviors the test suite must gate. Each test must (a) exist before its corresponding implementation ball lands and (b) fail for the right reason against pre-implementation code. The pattern is the same as SPEC-tracker-state §16: new-side assertions, not pinned old-binary fixtures.

1. **XDG paths from a fresh `bl init`.** A clone initialized with `bl init` has no `.balls/` directory at the clone root; on first `bl prime` the `~/.config/balls/<origin-key>/active/`, `~/.local/state/balls/<origin-key>/state-repos/own/`, and `~/.local/state/balls/<origin-key>/<path-hash>/worktrees/` paths exist and are correctly populated. `repo.json` exists on the own branch; `project.json` exists on the own branch; `tracker.json` does **not** exist on the own branch (solo case by default). No commit on `main` is created by `bl init` or `bl prime`.
2. **Origin-key derivation.** Given a fixed list of input URLs (HTTPS, SSH, with/without `.git`, with/without trailing slash, mixed case), the canonicalization and hash produce the documented `<origin-key>` values. Frozen golden vectors so a future implementation cannot drift.
3. **Per-clone isolation.** Two clones of the same origin into different directories share `state-repos/own/` (same `<origin-key>`) but have distinct `<path-hash>` subdirs; concurrent `bl claim` of two different tasks from the two clones writes to disjoint claim files and does not deadlock or corrupt either clone's locks.
4. **Bootstrap branch is constant.** `bl` does not consult any file at any path to determine the bootstrap branch name. Mutating `.balls/repo.json` or `.balls/tracker.json` to override the branch name has no effect — the binary fetches `origin balls/tasks` regardless. (The file *contents* may carry a `state_branch` for the *redirect* target; the bootstrap branch name itself is not configurable.)
5. **Redirect — single hop only.** A `tracker.json` on a repo's own branch with `state_url` set resolves to one federated tracker checkout. If that tracker's `balls/tasks` carries a `tracker.json` (synthetic test fixture), `bl prime` aborts with the "chained redirect" error.
6. **`tracker.json` is pointer-only.** A `tracker.json` carrying any field other than `state_url` and `state_branch` aborts `bl prime` with the "tracker.json schema: unexpected field" diagnostic (§6.1). `tracker.json` with neither `state_url` nor `state_branch` is also rejected ("tracker.json with no pointer").
7. **`tracker.json` absent on a solo repo.** A repo whose own `balls/tasks` carries no `tracker.json` resolves `<active-key> = own`; `repo.json` and `project.json` coexist on the same branch at distinct `.balls/` paths; `bl list`, `bl ready`, and `bl prime` all behave identically to a repo where `<active-key> != own`. The presence-or-absence of `tracker.json` is the sole structural difference.
8. **`repo.json` + `project.json` coexistence on a solo repo.** Both files live on the same branch at `.balls/repo.json` and `.balls/project.json`. A field that lives on `repo.json` (e.g. `integrate.mode`) is read from `repo.json`; a field that lives on `project.json` (e.g. `id_length`) is read from `project.json`. Per-file ownership holds even on the same branch.
9. **Precedence: repo wins over project for repo-owned fields.** A field set in both `repo.json` and `project.json` (e.g. `integrate.mode = direct` in repo, `forge-pr` in project) reads as the `repo.json` value. With only `project.json` set, the read returns the project's value. With neither set, the read returns the built-in default.
10. **Project-only fields are rejected in `repo.json`.** A `repo.json` containing `min_bl_version`, `id_length`, or `version` aborts the read with the documented diagnostic (§6.6). Test exercises one field of each, with the new-side assertion that the read errors rather than silently dropping or applying the value.
11. **Disjoint primary owners.** Programmatically: the union of `tracker.json`'s schema fields, `repo.json`'s schema fields, and `project.json`'s primary-owned schema fields has no field appearing in more than one of the three sets. The repo-fields-as-project-defaults overlap is enumerated explicitly and excluded from this check.
12. **Materialization is idempotent.** Running `bl prime` twice produces a `~/.config/balls/<origin-key>/` tree with identical inode-level state on the symlinks; no churn, no rebuilds, no error.
13. **State-repo regenerability.** Deleting `~/.local/state/balls/<origin-key>/state-repos/` and running `bl prime` rebuilds the checkouts from the tracker; only state-branch commits never pushed are lost (and only those — the test asserts pushed state survives).
14. **Worktrees relocated.** `bl claim bl-xxxx` creates a worktree at `~/.local/state/balls/<origin-key>/<path-hash>/worktrees/bl-xxxx/`. The clone's working tree contains no `.balls-worktrees/` directory and `.gitignore` contains no balls-related entries.
15. **`bl doctor` detects moved clone.** A clone whose `<path-hash>` no longer matches recorded per-clone state surfaces an orphaned-claims report; `bl repair --rebind-path` moves the per-clone state to the new hash with no data loss.
16. **`integrate.mode` rename gate.** `repo.json` carrying `integrate: { mode: direct }` exercises the local-squash code path; `integrate: { mode: forge-pr }` exercises the forge-deferred code path. The old field name `delivery` is rejected on a freshly-written `repo.json` (a file that contains `delivery` instead of `integrate` is treated as a bl-ed32 leftover that should have gone through `bl migrate`); the read either rejects, or — under Phase 1 dual-read — applies the rename in memory and warns. The test asserts whichever the phase under test specifies.
17. **`review.gate_command` rename gate.** `repo.json` carrying `review: { gate_command: "<cmd>" }` exercises the gate-command code path. The old field name `pre_check` is handled identically to the `delivery` case (Phase 1: dual-read + warn; Phase 2+: rejected post-migrate).
18. **No repo-level `target_branch`.** `repo.json` is rejected if it contains a top-level `target_branch` field. The resolution chain `task.target_branch ?? HEAD@root` is the only one exercised by `bl review`. A test that sets `HEAD@root` to `develop` and creates a task with no `target_branch` override delivers to `develop`; a test with `task.target_branch = main` delivers to `main` regardless of `HEAD@root`.
19. **No on-`main` writes (steady state).** Across a full lifecycle (`bl init`, `bl create`, `bl claim`, `bl review`, `bl close`) on a post-migration clone, `git log main` gains exactly the `[bl-xxxx]` delivery commit from `bl review` — and nothing else. No `balls: *` commits.
20. **Hand-operable join sequence.** The §10 shell sequence, executed with stock `git`, `ln`, `jq`, and `sha256sum`, produces a layout that subsequent `bl` invocations accept and operate on without further setup. The script covers both the solo branch (no `tracker.json`) and the federated branch (with `tracker.json`).
21. **Migration is one-shot — pre-XDG path.** Running `bl migrate` on a pre-XDG clone produces (a) the documented XDG tree with `repo.json`, `project.json`, and (if there was a redirect) `tracker.json`, (b) one commit on `main` removing the in-repo `.balls/` and `.gitignore` entries, and (c) no further state in the clone's working tree. Re-running `bl migrate` is a no-op.
22. **Migration is one-shot — bl-ed32 path.** Running `bl migrate` on a bl-ed32 clone splits `workspace.json` into `tracker.json` (only if `state_url` was set) + `repo.json` in one state-branch commit; applies the field renames; drops any repo-level `target_branch` with a warning. Re-running `bl migrate` is a no-op. No commit on `main`.
23. **Pre-XDG fallback never reappears (caveat asserted new-side).** New `bl` on a post-migration clone never creates `.balls/` in the clone's working tree, never reads from `.balls/config.json` on `main`, never reads or writes `.balls/workspace.json`, and never writes to `.balls-worktrees/` — the exact behaviors a pre-revision binary, ignorant of the three-file layout, would exhibit. Asserted on the new side rather than as an old-binary fixture: per SPEC-tracker-state §16.13, an old binary's behavior is immutable and a pinned-old fixture passes against pre-spec code and so cannot fail-then-pass across the refactor as the gating pattern requires.
24. **`min_bl_version` advisory.** On a clone whose `project.json` carries `min_bl_version > current`, `bl prime` prints a one-line upgrade warning and continues. (The warning lives on the project file, which a sufficiently old `bl` cannot read; this test asserts the warning emits for new `bl` running against a newer-`min_bl_version` project — the proactive channel works for upgraders.)

No phase ball under bl-f021 lands until its corresponding conformance tests exist and fail for the right reason against pre-implementation code.

## 15. Revision history

**bl-ed32 (commit aa26cc0, 2026-05-22)** — initial draft. Two files on the repo's own branch: `workspace.json` carrying both the redirect pointer and the per-code-repo config; `project.json` carrying the per-project config. `delivery.mode = local-squash | deferred`. `review.pre_check`. Repo-level `target_branch`. Filename: `SPEC-workspace-layout.md`.

**bl-dfd1 (commit 500b657, 2026-05-23)** — three files. `workspace.json` is split into `tracker.json` (pointer-only) + `repo.json` (per-code-repo). The split is structural rather than schema-only: presence-or-absence of `tracker.json` is the redirect signal at the filesystem level, and `repo.json` lives on every repo whether or not it redirects. Field renames: `delivery` → `integrate`, `review.pre_check` → `review.gate_command`. Repo-level `target_branch` removed (resolution chain becomes `task.target_branch ?? HEAD@root`). The bl-ed32 motivation, principles, worktree relocation, code/state split, and non-goals carry forward. Filename unchanged: `SPEC-workspace-layout.md`.

**bl-fc50 (this revision, 2026-05-23)** — terminology sweep. "Workspace" → "clone" for the per-on-disk-checkout concept; "workspace-owned" config (the `repo.json` layer) becomes "repo-owned" to match the file's own scope; the SPEC's §4 identity discussion is recast as a repo layer (origin URL) + clone layer (path within that repo). SPEC file renamed from `SPEC-workspace-layout.md` to `SPEC-clone-layout.md`. Cross-references in SPEC-tracker-state, SPEC-orphan-branch-state, SPEC-forge-gated-delivery, SPEC-lifecycle-sync-participants, README.md, SKILL.md, and code identifiers updated to match. The bl-1d46 sweep (commit d8bbf94) that settled on "workspace" is reversed here; the call was correct at the time but the XDG-layout pivot (bl-f021, bl-ed32) clarified the two-layer identity, and "workspace" overloads with multi-root toolings' use of the same word (VS Code workspaces, Cargo workspaces, npm workspaces). "Clone" is the natural noun for a specific on-disk checkout — it aligns with `git clone`, and the verb-noun ambiguity is absent in this usage ("this clone," not "do a clone"). No structural or schema change.
