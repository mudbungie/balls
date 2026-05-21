# balls

**balls** — **B**ranching **A**gent **L**abor and **L**ogistics **S**ystem — is a git-native task tracker for parallel agent workflows. Tasks are JSON files committed to your repo. Worktrees provide isolation. Git provides sync, history, and collaboration. There is no database, no daemon, no external service.

The CLI is `bl`. Every `bl` operation is expressible as file edits and git commands. The system is designed for a single developer running many agents, multiple developers each running many agents, and anything in between. It works offline. It degrades gracefully.

### Default workflow

One agent takes a task all the way through: `bl claim → work → bl review → bl close → done`. The `review` status is a transient checkpoint on the way to `closed`, not a handoff point. Balls does not assume a separate reviewer; if you want one, wire it up explicitly — otherwise the agent that submits also closes. This keeps agents from stopping short of finishing, which is the single most expensive failure mode in an agent-driven workflow.

Splitting submit and approve across two agents is supported (see SKILL.md → "Multi-agent: split submitter and reviewer") but is opt-in, not the default.

---

## Installation

Balls ships as a single small Rust binary called `bl`. The only runtime dependency is `git`.

### From crates.io

```bash
cargo install balls
bl completions --install
```

`cargo install` places `bl` in `~/.cargo/bin/` but cannot install shell completions on its own — `bl completions --install` writes bash, zsh, and fish completions to the standard `~/.local/share/...` paths.

### From source (recommended for development)

```bash
git clone https://github.com/mudbungie/balls.git
cd balls
make install
make hooks     # one-time: install the repo-local pre-commit hook
```

`make install` builds a release binary, installs `bl` to `~/.local/bin/` (plus a `balls` symlink so both names work on the command line), and installs shell completions to `~/.local/share/`. Make sure `~/.local/bin` is on your `PATH`.

`make hooks` wires up the repo-local pre-commit hook (clippy, line-length cap, tests, 100% coverage). Run it once per clone; it's not part of `make install` because a user installing the binary shouldn't have hooks attached to whatever repo they happen to be in. The coverage check requires `cargo install cargo-tarpaulin`.

`make hooks` is recommended, not required. A pre-commit hook and a **bare core repo** are two valid paths to the same guarantee — that the 300-line and 100%-coverage gates can't be bypassed — and which one fits depends on circumstance:

- **Local hook** — for an ordinary clone where the working branch can be committed to directly, when you want the gate to fail at commit time, or when there is no CI. Strength: fast local feedback. Cost: a per-clone install, a `tarpaulin` run on every commit, and `git commit --no-verify` slips past it.
- **Bare core** — for the worktree/merge model this repo uses: a bare core, every change arriving via a worktree and a `bl review` squash-merge, the gates enforced in CI. A bare repo has no working tree, so the working branch *cannot* be edited directly — the architecture makes the bypass impossible rather than merely discouraging it. Cost: a violation surfaces at review/CI, not at the commit. Standing one up is *The bare central hub → Bootstrapping a bare hub from scratch*, below.

The two compose rather than exclude: a bare core can still install the hook (one install in the shared common dir covers every worktree) for at-commit feedback layered on top of the structural guarantee.

A third path covers the gap the first two leave. Both the hook and CI miss `bl review`'s squash *itself* — balls makes that commit with `git commit --no-verify`, and CI only runs once it has already landed. Setting **`review.pre_check`** in `.balls/config.json` makes `bl review` run a command (typically `make check`) against the merged worktree *before* the squash and abort the delivery if it fails — the gate fires at the moment the merge happens. See *Delivery Modes → Pre-squash review gate*.

To remove everything `make install` placed:

```bash
make uninstall
```

### Cross-compilation

```bash
cargo install cargo-zigbuild
cargo zigbuild --release --target x86_64-unknown-linux-gnu
cargo zigbuild --release --target aarch64-unknown-linux-gnu
cargo zigbuild --release --target x86_64-apple-darwin
cargo zigbuild --release --target aarch64-apple-darwin
```

### Planned (not yet available)

- Prebuilt binaries: `curl -fsSL https://github.com/mudbungie/balls/releases/latest/download/bl-$(uname -s)-$(uname -m) -o /usr/local/bin/bl && chmod +x /usr/local/bin/bl`
- Homebrew tap

### Verify

```bash
bl --version
cd your-repo
bl init
bl create "My first task"
bl list
```

Balls is MIT licensed. See `LICENSE`.

### Environment variables

| Variable | Purpose | Default |
|---|---|---|
| `BALLS_IDENTITY` | Worker identity for claim/close/prime operations | `$USER`, then `"unknown"` |

#### Tip: unique identities for agent sessions

If you're running balls under an LLM-driven agent, don't ask the model to invent its own identity — language models are not RNGs and collapse to the same handful of names across sessions (you will end up with three Junipers stepping on each other's claims). Source the randomness outside the model: have the agent harness pick a name at session start and inject it as `BALLS_IDENTITY`. A portable recipe is `shuf -n1 /usr/share/dict/words` (or `petname` if you want adjective-noun pairs).

In Claude Code, this is a `SessionStart` hook in `~/.claude/settings.json` that prints a JSON payload with `hookSpecificOutput.additionalContext` setting the name; other harnesses typically expose an equivalent pre-session shell hook that can `export` the variable directly.

### Library usage

Ball is also available as a Rust library crate for programmatic integration:

```rust
use balls::{Store, Task};

let store = Store::discover(&std::env::current_dir().unwrap()).unwrap();
for t in balls::ready::ready_queue(&store.all_tasks().unwrap()) {
    println!("[P{}] {} {}", t.priority, t.id, t.title);
}
```

---

## Releasing

Releases to [crates.io](https://crates.io/crates/balls) are automated via [release-plz](https://release-plz.dev/) and GitHub Actions. The normal flow:

1. Merge feature PRs to `main` using the project's usual commit style — a short title with a `[bl-xxxx]` trailer, optionally followed by a body. Every non-`balls:` commit is picked up by release-plz's changelog.
2. On every push to `main`, `.github/workflows/release-plz.yml` opens (or updates) a **Release PR** that bumps `Cargo.toml`, regenerates `CHANGELOG.md`, and lists the commits going into the release.
3. Review the Release PR. CI (`.github/workflows/ci.yml`) runs `cargo test`, `cargo clippy`, line-length + 100% coverage checks, and `cargo publish --dry-run` against it.
4. Merge the Release PR. release-plz tags `vX.Y.Z`, creates a GitHub release, and publishes to crates.io.

### Bump signaling

Source commits here aren't Conventional Commits, so release-plz can't infer minor/major from `feat:` or `fix:` prefixes. Instead, `release-plz.toml` configures bracketed markers (via `custom_minor_increment_regex` / `custom_major_increment_regex`) that compose with the existing `[bl-xxxx]` style:

| Marker      | Bump  | When to use                                                                 |
|-------------|-------|------------------------------------------------------------------------------|
| `[major]`   | major | Breaking change — task file format, CLI flag removed, etc.                  |
| `[minor]`   | minor | New user-visible capability — new command, new config option, new behavior. |
| *(none)*    | patch | Default. Bugfix, refactor, doc, internal cleanup.                           |

Put the marker anywhere in any commit's message that lands in the release window — release-plz matches the regex against the full commit text, so the subject (alongside `[bl-xxxx]`) or the body both work. The marker must be a standalone bracketed token (preceded by start-of-string or whitespace, followed by end-of-string or whitespace), so prose mentions like `[minor]/[major]` describing the convention don't self-trigger a bump. The most prominent commit of the change is the natural place. If multiple commits in the window are marked, the highest bump wins.

```
bl review bl-abcd \
  -m "Add forge-gated delivery mode [minor]" \
  -m "Opt-in via delivery.mode = \"deferred\" in .balls/config.json…"
```

If you forget the marker and release-plz proposes a patch when you wanted minor/major, edit the Release PR's `Cargo.toml`/`Cargo.lock`/`CHANGELOG.md` and title before merging (the pre-0.4 hand-edit workflow is the escape hatch).

Commit-parser and semver-check behavior is configured in `release-plz.toml` at the repo root.

### One-time setup

- Add a crates.io API token as the repo secret `CARGO_REGISTRY_TOKEN` (Settings → Secrets and variables → Actions). Scope it to `publish-update` for this crate.
- Under Settings → Actions → General → Workflow permissions, allow GitHub Actions to create and approve pull requests.

### Manual release (fallback)

If you need to cut a release without release-plz:

```bash
# on main, with a clean tree
cargo test && cargo publish --dry-run
# bump version in Cargo.toml, update CHANGELOG.md
git commit -am "Release vX.Y.Z"
git tag vX.Y.Z
git push origin main --tags
cargo publish
```

---

## Principles

1. **Git is the database.** Task files are committed, pushed, pulled, and merged like code, on a dedicated orphan ref inside your existing repo. No external storage engine.
2. **Main stays clean.** Balls bookkeeping lives on the `balls/tasks` orphan branch. `git log --oneline main` reads as a changelog — one feature commit per delivered task, tagged with the task ID.
3. **One file per task.** Atomic unit of state. Merge conflicts are per-task, and text-mergeable schema makes most conflicts disappear entirely.
4. **Derived state is computed, never stored.** Completion percentages, ready queues, dependency trees — all calculated at read time. The one exception is `delivered_in`, an explicit self-healing cache backed by the delivery tag.
5. **Local cache is disposable.** The `.balls/local/` directory is gitignored ephemeral state. Deleting it loses nothing durable.
6. **Offline-safe.** All operations produce valid local state. Conflicts are resolved at merge time, never prevented by connectivity checks.
7. **Worktrees are first-class.** Claiming a task creates a git worktree. The worktree name is the task ID. One task, one workspace.
8. **The CLI is a convenience, not a requirement.** Every operation is expressible as file edits + standard git commands. A human with `vim`, `ln`, and `git` can do everything `bl` does — `SPEC-orphan-branch-state.md` §11 publishes the shell sequences.
9. **Plugins extend, core stays small.** External integrations (Jira, Linear, GitHub Issues) are handled by a plugin interface. Auth, sync logic, and API specifics never enter the core.

---

## Glossary

| Term | Meaning |
|---|---|
| **task** | A unit of work. One JSON file on the `balls/tasks` orphan branch, exposed to main via a symlink. |
| **state branch** | The orphan git branch `balls/tasks` that holds all task state. No shared history with main. |
| **state worktree** | A second git worktree at `.balls/worktree/` with the state branch checked out. Where task files physically live. |
| **bare hub** | The recommended deployment: a bare repo (`core.bare = true`) with no work tree. All work happens in `.balls-worktrees/<id>/` checkouts; direct commits to the operating branch are a git-level impossibility, not a discouraged convention. Its "repo root" is the bare gitdir's parent. See *The bare central hub* below. |
| **ready** | A task that is open, has all dependencies met, and is unclaimed. |
| **claim** | Taking ownership of a task. Creates a git worktree under `.balls-worktrees/<id>/` for the work. |
| **review** | Squash-merges the work branch into main as a single feature commit tagged `[bl-xxxx]` and flips the task to `review` on the state branch. A checkpoint state; the default flow is to follow it with `bl close`. |
| **close** | Finishes the task: archives it on the state branch and removes the bl worktree. Run from the repo root by whichever agent is finishing the task — the same one that submitted, or a separate reviewer if one is configured. |
| **drop** | Releasing a claim. Destroys the bl worktree and resets the task to `open`. |
| **sync** | Fetch + merge + push both main and the state branch against the git remote. |
| **delivery tag** | The `[bl-xxxx]` token embedded in a review's main-branch commit subject. Ground truth for which commit delivered a task. |
| **plugin** | An external executable that implements the plugin interface for a specific integration (e.g., Jira). |

---

## Architecture: the state branch

Balls stores task state on a dedicated orphan git branch called `balls/tasks`. It has no shared history with your project's `main` — it's a parallel ref that lives in the same repo, next to your code, managed by the same git. This is the load-bearing design choice. Every consequence below flows from it.

**Why an orphan branch.** The alternative would be to commit task files directly to `main`, which is what most git-native task trackers do. That approach adds a commit to main every time anyone claims, reviews, closes, or notes a task — so `git log --oneline main` becomes half feature commits and half task bookkeeping. Balls moves the bookkeeping off main entirely. Your main history reads like a changelog: one clean feature commit per delivered task, no noise.

**Why still a git ref.** An external database (SQLite, Dolt, a TOML file outside the repo) would also keep main clean. But then your task state is separate from your code state — two things to back up, two things to sync, two mental models. An orphan git ref stays inside the repo you already have. `git clone` fetches it. `git push` publishes it. `git log balls/tasks` reads its history with tools every developer already knows. No new infrastructure.

**Naïve visibility.** Because task state is ordinary git data, a contributor who doesn't know balls exists can still read it. `ls .balls/tasks/` shows task files. `cat .balls/tasks/bl-abc.json` prints JSON. `jq` and `grep` and their editor's file tree all work. The CLI is a convenience; everything balls does is expressible as standard git + file operations.

### File and Folder Layout

Two deployment shapes share one store format. The **ordinary clone** has
a working tree; the **bare central hub** has none. The state worktree,
the `.balls/tasks` symlink, the orphan branch, and `.balls-worktrees/`
are byte-for-byte identical in both — bare-ness changes only the root,
never the orphan-branch machinery.

**Ordinary clone (working tree present):**

```
project/
├── .balls/                          # gitignored on main, set up by `bl init`
│   ├── tasks → worktree/.balls/tasks    # symlink — naïve view into the state branch
│   ├── worktree/                        # git worktree on the orphan `balls/tasks` branch
│   │   └── .balls/tasks/
│   │       ├── bl-a1b2.json             # tracked on balls/tasks, not on main
│   │       ├── bl-a1b2.notes.jsonl      # append-only notes sidecar
│   │       └── .gitattributes           # activates merge=union for notes files
│   ├── config.json                      # committed to main (project-wide settings)
│   ├── plugins/                         # committed to main (plugin configs)
│   │   └── jira.json
│   └── local/                           # gitignored ephemeral state (per-clone)
│       ├── claims/                      # one file per active local claim
│       ├── lock/                        # flock files, incl. state-worktree.lock
│       └── plugins/                     # plugin runtime state (tokens, caches)
├── .balls-worktrees/                    # gitignored; `bl claim` creates worktrees here
│   ├── bl-a1b2/                         # full checkout on work/bl-a1b2 branch
│   └── bl-c3d4/
└── ... (project files on main)
```

The `.balls/tasks` symlink in main's working tree is the key to naïve visibility. It points at `.balls/worktree/.balls/tasks`, which is the state worktree's checkout — where task files physically live. Reading `.balls/tasks/bl-abc.json` follows the symlink into the state worktree and returns the canonical file. `bl` commands and hand-editing agree.

**Bare central hub (no working tree — the recommended deployment):**

```
hub/                                 # "repo root" = the bare gitdir's parent
├── .git/                            # bare gitdir (core.bare = true); no checkout
├── .balls/                          # loose on-disk store — NOT a tracked working set
│   ├── tasks → worktree/.balls/tasks    # symlink — same as the ordinary clone
│   ├── worktree/                        # state worktree on balls/tasks (unchanged)
│   │   └── .balls/tasks/
│   │       ├── bl-a1b2.json
│   │       ├── bl-a1b2.notes.jsonl
│   │       └── .gitattributes
│   ├── config.json                      # project-wide settings (materialized here)
│   ├── plugins/                         # plugin configs
│   └── local/                           # ephemeral per-hub state
│       ├── claims/
│       ├── lock/
│       └── plugins/
└── .balls-worktrees/                    # the only working trees on a bare hub
    ├── bl-a1b2/                         # full checkout on work/bl-a1b2 branch
    └── bl-c3d4/
```

At a bare hub there is no `project/` working tree, and nothing is "gitignored on main" — there is no checked-out main to ignore from. `.balls/` and `.balls-worktrees/` are the store itself, ordinary directories sitting next to the bare gitdir, not a tracked working set; `config.json` and `plugins/` are present as loose files rather than a checkout of main. The only real working trees are the per-task `.balls-worktrees/<id>/` checkouts. The state worktree, the `.balls/tasks` symlink, and the state branch are exactly as in the ordinary clone. How the loose store comes to exist at a fresh bare root is the bootstrap sequence — see *The bare central hub* and the `bl init` section.

### .gitignore entries

`bl init` adds these to the **ordinary clone's** `.gitignore` on main:

```
.balls/local
.balls/tasks
.balls/worktree
.balls-worktrees
```

A bare hub has no checked-out main and no `.gitignore` governing it: the same paths are never a tracked working set there, so nothing has to be ignored. These entries matter only for the working-tree case.

### State branch history

```
main                                balls/tasks  (orphan — no shared history)
  |                                       |
  <feature commit> [bl-a1b2]               balls: create bl-a1b2
  <feature commit> [bl-c3d4]               balls: claim bl-a1b2
                                          state: review bl-a1b2
                                          state: close bl-a1b2 - title
                                          balls: create bl-c3d4
                                          ...
```

Every lifecycle transition (create, claim, review, close, update, note, dep, link) is a commit on `balls/tasks`. The only commits that land on `main` are the substantive feature commits produced by `bl review` — each one carries a `[bl-xxxx]` delivery tag in its subject so the main commit can be correlated back to the state-branch record. See the Delivery Link section below.

---

## The bare central hub (recommended deployment)

The recommended production topology is a **bare** repository (`core.bare = true`) acting as a central hub. There is no working tree at the root: every change arrives through a `bl claim` worktree under `.balls-worktrees/<id>/` and a `bl review` squash-merge. This is the deployment this very repo uses; `bl init --bare <source> <hubdir>` stands one up in a single command, with the *Bootstrapping a bare hub from scratch* subsection below documenting both it and the equivalent by-hand sequence.

**Why bare-ness is load-bearing, not incidental.** The rest of these docs preach a worktree-only rule — *edits go in the worktree, never directly on the operating branch*. A pre-commit hook can only *discourage* the bypass; `git commit --no-verify` slips past it. A bare repo makes the bypass a **git-level impossibility**: a bare repo has no working tree, so git itself refuses to commit or even stage on the operating branch from the root (`fatal: this operation must be run in a work tree`). The convention becomes a hard invariant that no agent or human can violate, by mistake or on purpose. That structural guarantee — not mere tidiness — is why bare is the recommended hub.

**Where the root is, and what's in it.** balls resolves the repo root of a bare hub as the bare gitdir's parent directory (`find_main_root`). That directory holds the bare `.git`, plus the on-disk store directories — `.balls/` and `.balls-worktrees/` — as ordinary files. They are *not* a tracked working set; they are the store itself, sitting next to the bare gitdir.

**Observing state — `git status` at the root is fatal by design.** At a bare root, `git status` does not print a clean tree; it exits with `fatal: this operation must be run in a work tree`. That is correct behavior, not a broken repo: there is no work tree to report on, and the loose files at the root are the store, not git's working set. Read state the two ways that *do* work:

- **Tasks:** `bl list` (and `bl show`, `bl ready`, `bl prime`) — all read-only, all run from the bare root.
- **Code in flight:** `git status` / `git diff` / `git log` *inside* the active `.balls-worktrees/<id>/` checkout, which is an ordinary worktree where those commands behave normally.

**Where `bl` must run.** As of bl-8cf7, `Store::discover` tolerates a bare root, so every read-only and root command works from the bare hub directly. (Before that fix they failed with a misleading `not initialized. Run bl init` even on a healthy hub; bl-597e additionally made discovery errors path-aware so the wrong-directory case is obvious.) Two commands have bare-specific mechanics, both transparent to the caller:

- `bl review` cannot run a working-tree squash at a bare root, so it provisions an ephemeral detached worktree under `.balls/local/`, performs the squash there, and fast-forwards the operating branch from the bare gitdir afterward (see `bare_squash.rs`). You invoke it exactly as on a normal repo.
- `bl close` runs from the bare root normally. Its only hard constraint is that it must **not** be run from inside the bl worktree it is about to delete — and a bare hub's root is, by construction, never inside a worktree.

**Reconciling "run from the repo root."** Every "run `bl close` from the repo root" instruction in this README and in SKILL.md is correct for a bare hub: the bare directory *is* that repo root, and `bl close` prints it on success so you can `cd` back. The rule was never "find the checked-out main branch" — it is "not from within the bl worktree." On a bare hub there is no checked-out main to stand in; that absence is the entire point of choosing bare.

### Bootstrapping a bare hub from scratch

**The one-liner: `bl init --bare`.** Once the project's `balls/tasks` orphan branch is published (step 1 below — a no-op if the project already uses balls), one command stands the hub up:

```bash
bl init --bare git@host:proj.git /srv/proj-hub
```

It bare-clones the source into `/srv/proj-hub/.git`, wires the `origin` fetch refspec, reconstructs the loose store (the `.balls/` scaffolding, the `.balls/tasks` symlink, `config.json` materialized from `main`'s tree), and attaches the state worktree. It is idempotent and non-destructive in exactly the way the working-tree `bl init` is: re-running it reuses an existing bare gitdir (and refuses to clobber a *non*-bare `.git` there), re-creates only what is missing, and never force-pushes or resets a shared branch. The source's `main` must already be balls-initialized; if it has no `.balls/config.json` the command stops with that message rather than guessing.

**The by-hand sequence (still canonical).** `bl init --bare` is a convenience wrapper over standard git plumbing, not a new primitive — per the orphan-branch design principle that standard tools must suffice, the manual sequence below remains valid and is what the wrapper mechanizes (steps 2–3). Use it when you want to see exactly what the hub is, or when scripting around a constraint the wrapper doesn't cover. The sequence is short because the orphan-branch design means a hub is just a loose store wrapped around an already-published `balls/tasks`:

```bash
# 1. ONE working-tree clone creates the balls/tasks orphan branch and
#    pushes it to the shared remote. Skip this whole step if the project
#    already uses balls — balls/tasks is already on origin.
git clone git@host:proj.git /tmp/proj-init && cd /tmp/proj-init
bl init        # writes `balls: initialize` on main, creates + pushes balls/tasks
bl sync        # ensure both main and balls/tasks are on origin
cd / && rm -rf /tmp/proj-init        # the seeding clone is disposable

# 2. The hub is a BARE repo whose gitdir is a `.git` directory inside
#    the hub root, so the gitdir's parent (the hub root) is where the
#    loose store lives. Clone bare into that `.git`.
mkdir -p /srv/proj-hub
git clone --bare git@host:proj.git /srv/proj-hub/.git
cd /srv/proj-hub
git --git-dir=.git config remote.origin.fetch '+refs/heads/*:refs/remotes/origin/*'
git --git-dir=.git fetch origin

# 3. Reconstruct the loose store at the bare root — the part `bl init`
#    would do in a working-tree clone but cannot here. The store dirs are
#    gitignored on main so a clone never carries them; config.json IS
#    tracked on main, so materialize it from main's tree (no checkout
#    exists to copy it from at a bare root).
git worktree add .balls/worktree balls/tasks
ln -s worktree/.balls/tasks .balls/tasks
mkdir -p .balls/plugins .balls/local/claims .balls/local/lock .balls/local/plugins
git show main:.balls/config.json > .balls/config.json

# 4. Verify. Read-only and root commands work from the bare root (bl-8cf7).
bl list
```

The state worktree added in step 3 already carries `.gitattributes` and every task file from `balls/tasks`, so nothing is reseeded — it is pure on-disk scaffolding around an already-populated orphan branch. From here `bl claim` / `bl review` / `bl close` run at the bare root exactly as described above; `bl review`'s ephemeral-worktree squash (`bare_squash.rs`) needs no extra setup.

---

## Delivery Modes

`bl review` delivers a task's work branch onto an **integration branch**. *Which* branch, and *how* the squash lands, are two axes balls makes explicit. The full design — invariants, backwards-compat audit, conformance tests — is [docs/SPEC-forge-gated-delivery.md](docs/SPEC-forge-gated-delivery.md); this section is the operational summary.

**The integration branch is resolved, not assumed.** At review time:

```
effective_target_branch = task.target_branch       # per-task override
                       ?? config.target_branch      # repo-level setting
                       ?? git_current_branch(root)   # the historical fallback
```

The last fallback — *whatever branch is checked out at the repo root* — is what balls has always done, and it lived undocumented in `review.rs` until this section. It is still the default: a repo that sets neither `config.target_branch` nor a per-task `target_branch` behaves bit-identically to every prior version, squashing into the root's current branch. Setting `target_branch` only matters when you want to stop depending on "whatever HEAD points to" — e.g. a git-flow repo whose features target `develop` while a hotfix task overrides `target_branch` to `main`.

### local-squash (default)

`bl review` squashes `work/bl-xxxx` into the integration branch immediately and locally, writes the `delivered_in` hint, and flips the task to `review`. One agent then runs `bl close` and the task is done. This is the trunk-based flow the rest of these docs describe; nothing changes unless you opt in below.

**Worked example — single-agent, trunk-based:**

```bash
bl claim bl-a1b2                       # -> .balls-worktrees/bl-a1b2
cd .balls-worktrees/bl-a1b2
# ...edit, commit...
bl review bl-a1b2 -m "Add rate limiter"   # squashes into main locally, status=review
cd <repo root>
bl close  bl-a1b2 -m "ship"             # main now carries the squash with [bl-a1b2]
```

### deferred (forge-gated, opt-in)

For repos whose merges are produced *by a forge* after required review/CI — GitHub PRs, GitLab MRs, Gitea PRs. Enabled per-repo in config (`delivery.mode = "deferred"`); `target_branch` must be set explicitly (a PR needs an unambiguous base). In this mode `bl review`:

1. Pushes `work/bl-xxxx` to `origin` instead of squashing locally.
2. Auto-creates a **gate child** task and links it `parent gates child`.
3. Flips the parent to `review` — but leaves `delivered_in` null and the integration branch untouched.

The parent's `bl close` is now blocked by the open `gates` link (an existing primitive — see *Gates: post-review blockers*). It unblocks only when the gate child closes, which happens when the forge merges the PR — done by hand, or automatically by a **forge plugin** (see below). The `[bl-xxxx]` tag on the integration branch is then the forge-produced merge commit; `bl close` resolves `delivered_in` via tag-scan.

**Worked example — agent + forge PR:**

```bash
bl claim bl-c3d4 && cd .balls-worktrees/bl-c3d4
# ...edit, commit...
bl review bl-c3d4 -m "Add OAuth flow"
#   pushes work/bl-c3d4 to origin, prints a recommended PR title ending [bl-c3d4]
#   and the gate child id; parent is now review + gated
gh pr create --base develop --head work/bl-c3d4 --title "Add OAuth flow [bl-c3d4]"
# ...reviewers approve, forge merges the PR into develop...
# the gate child closes (manually, or a forge plugin's sync closes it)
cd <repo root>
bl close bl-c3d4 -m "ship"             # unblocked; delivered_in resolved by tag-scan
```

### Pre-squash review gate

Both modes above can run a project-defined check before `bl review` delivers. Set it in committed `.balls/config.json`:

```json
{ "review": { "pre_check": "make check" } }
```

`bl review` runs `pre_check` once it has committed the worker's work and merged the integration branch into the worktree — so the check sees the exact end-state being delivered — and *before* the squash (local-squash) or the branch push (deferred). A non-zero exit aborts the review: no squash, no push, no status flip. The integration-branch merge stays in the worktree, so you fix the failure there and re-run `bl review`; the check's own output streams to your terminal.

This is where a repo's quality gate belongs. balls commits every state-branch write — and the squash itself — with `git commit --no-verify`, so a git `pre-commit` hook structurally cannot see the merge to the integration branch, and CI sees it only after it has landed. `pre_check` is the one gate that runs *at* the merge. Because it lives in committed config, a `master_url` hub enforces it across the whole federation. Unset (the default) ⇒ no gate, byte-identical to before.

### Backwards-compatibility caveat

All new behavior is opt-in via config; new fields use lenient serde, so an **old `bl` reading a deferred-mode repo silently ignores the new fields**. The one accepted hazard: an old `bl` running `bl review` on a deferred-mode repo does not know to defer — it performs the old local squash, contaminating the integration branch with a premature commit. This is documented, not engineered against (per project decision 2026-05-10); a repo can advertise a `min_bl_version` so newer clients warn. The more dangerous case — an old client tearing a gated task down mid-review — *is* prevented: old `bl close` already respects the `gates` block, because gates predate this feature.

### Forge plugins vs. issue-tracker plugins

A **forge plugin** (e.g. a GitHub PR plugin) automates the deferred-mode gate: it opens/updates the PR on `bl review` and closes the gate child when the PR merges. This is a *different role* from an **issue-tracker plugin** (Jira, Linear, GitHub Issues), which mirrors task state to/from an external backlog. They use the same plugin protocol (§Plugin System) but solve unrelated problems — don't reach for a forge plugin to sync issues, or an issue-tracker plugin to gate delivery. Forge plugins are per-forge and ship **separately** from balls core (the lifecycle hooks here are forge-agnostic; only the plugin is forge-specific); a concrete GitHub implementation is tracked as its own deliverable, not bundled in this repo.

---

## Task File Schema

Each task is a single JSON file at `.balls/tasks/<id>.json`.

```json
{
  "id": "bl-a1b2",
  "title": "Implement auth middleware",
  "type": "task",
  "priority": 1,
  "status": "open",
  "parent": null,
  "depends_on": ["bl-x9y8"],
  "description": "Add JWT validation middleware to all API routes.",
  "created_at": "2026-04-09T14:00:00Z",
  "updated_at": "2026-04-09T14:00:00Z",
  "closed_at": null,
  "claimed_by": null,
  "branch": null,
  "tags": ["auth", "api"],
  "links": [{"link_type": "relates_to", "target": "bl-z7w6"}],
  "closed_children": [],
  "external": {},
  "delivered_in": null,
  "repo": "git@github.com:you/project.git"
}
```

The fields with no value here are *omitted* from the file rather than written as `null`/`{}`: `synced_at`, `sync_status`, `delivered_repo`, `target_branch`, and any `extra` passthrough keys appear only once set, and `repo` is likewise omitted when the task's code origin is unknown. The block above is a freshly-created task; see the table below for every field a task file can carry.

Notes live in a sibling file `<id>.notes.jsonl` rather than in the task.json. That split is an architectural invariant — see Text-Mergeable Schema below.

### Field definitions

| Field | Type | Description |
|---|---|---|
| `id` | string | Format `bl-XXXX` (4 hex chars by default). Generated from sha1 of title + timestamp, truncated. |
| `title` | string | Human-readable summary. |
| `type` | string | Free-form identifier label. Common values: `task`, `bug`, `epic`, `feature`, `chore`, `spike`, `question`, `discussion`, `retro`. Only `epic` has behavioral meaning (progress bar, `[epic]` marker). `bl create -t` accepts any `[a-z][a-z0-9_-]*`. |
| `priority` | int | 1 (highest) to 4 (lowest). |
| `status` | enum | `open`, `in_progress`, `review`, `blocked`, `closed`, `deferred`. |
| `parent` | string? | ID of parent epic/task, or null. |
| `depends_on` | string[] | IDs of tasks that must close before this is workable. |
| `description` | string | Full description. |
| `created_at` | ISO 8601 | Creation timestamp. |
| `updated_at` | ISO 8601 | Last modification timestamp. |
| `closed_at` | ISO 8601? | When closed, or null. |
| `claimed_by` | string? | Worker identity string, or null. |
| `branch` | string? | Git branch name for this task's work, or null. |
| `tags` | string[] | Freeform labels. |
| `links` | object[] | Typed relationships: `{"link_type": "relates_to\|duplicates\|supersedes\|replies_to", "target": "bl-XXXX"}` |
| `closed_children` | object[] | Archived child tasks: `{"id": "...", "title": "...", "closed_at": "..."}`. Populated when a child task is closed and archived. |
| `external` | object | Plugin-managed foreign keys. e.g., `{"jira": {"key": "PROJ-123", "synced_at": "..."}}`. Core never reads this; plugins own it. |
| `synced_at` | object | Per-plugin timestamp of the last applied sync response: `{"<plugin>": "ISO 8601"}`. Plugins compare it against their remote's `updated_at` for bidirectional conflict resolution. A missing key means that plugin has never synced the task. Omitted from the file when empty. |
| `sync_status` | object | Per-plugin verbatim reason the last native sync negotiation was skipped or failed: `{"<plugin>": "reason"}`. Set on skip, cleared on the next success. Omitted from the file when empty. |
| `delivered_in` | string? | SHA of the main-branch squash commit that delivered this task. Written by `bl review`. Performance hint only — ground truth is the `[bl-xxxx]` tag in the commit subject. See Delivery Link. |
| `repo` | string? | Code-home provenance: the code repo this task's work belongs to, as a fetchable `origin` URL. Stamped by `bl create`, re-anchored to the claiming clone by `bl claim`. Only a real URL is auto-written, so null/omitted means "origin unknown," not "single-repo." |
| `delivered_repo` | string? | Delivery provenance: the code repo whose history contains `delivered_in`. Set wherever `delivered_in` is. Distinct from `repo` when a task is created in one clone and delivered from another on a shared hub. Null/omitted means the locally checked-out repo. |
| `target_branch` | string? | Per-task override of the repo-level `target_branch` config. When set, `bl review` squashes this task into this branch, ignoring the repo default and current-branch fallback. Omitted when unset. |
| `extra` | object | Forward-compat passthrough. Any top-level JSON key the current `bl` doesn't recognize lands here on load and round-trips back out on save, so an older `bl` won't silently drop a field a newer one wrote. Flattened into the top-level object — not nested under an `extra` key — and omitted when empty. |

### Text-mergeable schema

Task files are serialized with a specific shape that lets stock `git merge` handle most collisions without a custom merge driver:

- Top-level keys are sorted alphabetically.
- Each field sits on its own line with a compact single-line value.
- Trailing newline; no pretty-printed nested objects.

The consequence is that two workers editing different fields of the same task produce non-overlapping diffs and merge cleanly. Two workers editing the *same* field of the same task produce a real conflict that `bl sync` surfaces and auto-resolves via field-wise precedence (see Conflict Resolution).

Notes are split out to `<id>.notes.jsonl` — an append-only JSON Lines file — and marked `merge=union` in `.gitattributes`. Two workers appending different notes to the same task merge cleanly at the line level, no resolver needed. Deleting a task (via archive) removes both the `.json` and the `.notes.jsonl` in the same commit.

### ID generation

```
echo -n "${title}${timestamp}" | sha1sum | cut -c1-4 | sed 's/^/bl-/'
```

ID length is configurable in `.balls/config.json` (`id_length`, clamped to 4..=32). On collision, a fresh timestamp is tried.

### Delivery link

`bl review` squash-merges the worker's branch into main and commits a single feature commit whose subject ends with `[bl-xxxx]`. It then writes that commit's SHA into the task's `delivered_in` field on the state branch — a cache for fast lookup. The ground truth is the tag in the commit subject, which survives rebase, amend, cherry-pick, and filter-branch. On read, `bl show` verifies the hint and falls back to `git log -F --grep '[bl-xxxx]' main` if the SHA has drifted (stale cache marked explicitly in `bl show --json`).

---

## Derived State (computed, never stored)

### Ready queue

A task is **ready** if:
- `status` == `open`
- All IDs in `depends_on` refer to tasks with `status` == `closed`
- `claimed_by` is null

### Group completion

For a parent task, completion = (`closed_children` count + live children with `status == "closed"`) / (total children including archived). Children are tasks where `parent == this task's id`. `closed_children` on the parent tracks archived children.

### Dependency-blocked

A task is dependency-blocked if any ID in `depends_on` refers to a task with `status` != `closed`. A missing dependency (task file deleted after archival) is treated as closed, not blocked.

### Task archival

When a task is closed, its `.json` and `.notes.jsonl` files are removed from the state branch's HEAD via a single `state: close bl-xxxx` commit. The full task data is preserved in git history — `git show balls/tasks~N:.balls/tasks/bl-xxxx.json` retrieves any past version. If the archived task had a parent, the parent's `closed_children` array is updated in the same commit. This keeps the working set small: only live tasks exist in the state branch tip.

---

## Local Cache (.balls/local/)

Per-clone ephemeral state. Gitignored, disposable, rebuilt by `bl init`.

### lock/

Advisory flocks serializing local writes:

- `lock/<task-id>.lock` — one file per task, held by any write path for that task. Prevents two workers on the same machine from racing a claim or update.
- `lock/state-worktree.lock` — store-wide lock held during any write to the state worktree (`commit_task`, `commit_staged`, `remove_task`, `close_and_archive`). Serializes concurrent bl invocations from different tasks so git's `index.lock` in `.balls/worktree/` never sees contention. This is the lock that makes parallel agent swarms safe.

Both locks use `flock(2)`: if another process holds the lock, the caller blocks until it's released. No polling, no races.

### claims/

One file per active local claim. Filename is the task ID. Contents:

```
worker=dev1/agent-alpha
pid=48291
claimed_at=2026-04-09T15:00:00Z
```

This is a performance shortcut for fast local double-claim prevention. The source of truth is `claimed_by` in the state-branch task file.

### plugins/

Plugin auth tokens and runtime caches, scoped per plugin name. Plugins own this directory entirely — balls never reads it.

---

## Worktree Lifecycle

### Claim

`bl claim bl-a1b2` acquires the per-task flock, flips the task's status to `in_progress` and writes `claimed_by`/`branch` fields on the state branch, commits that change (`balls: claim bl-a1b2 - title`), then creates a git worktree at `.balls-worktrees/bl-a1b2/` on a fresh `work/bl-a1b2` branch. The bl worktree is symlinked to share `.balls/local`, `.balls/worktree`, and `.balls/tasks` with main so task state is visible from inside it. Prints the worktree path on success.

None of this touches main. The claim commit lands on `balls/tasks`, not on your project's history.

### Work

The worker edits files inside `.balls-worktrees/bl-a1b2/`, committing to `work/bl-a1b2` with regular `git add`/`git commit`. The bl worktree is an ordinary git checkout — editors, build tools, and tests all work normally.

### Review

`bl review bl-a1b2 -m "Short title\n\nBody paragraph..."` is the worker's exit point. It:

1. `git add -A && git commit -m "wip: bl-a1b2"` in the bl worktree to sweep up any uncommitted changes.
2. Merges main into the bl worktree (forward merge). If this step has conflicts, review fails — resolve them in the worktree and try again.
3. Squash-merges `work/bl-a1b2` into main as a single feature commit. The title is the first line of `-m`, `[bl-a1b2]` is appended, and the rest becomes the commit body. This is the one and only commit on main for this task.
4. Captures the new main HEAD SHA into the task's `delivered_in` field.
5. Flips the task's status to `review` on the state branch and commits both the status change and the delivery hint in one `state: review bl-a1b2` commit.
6. Merges main back into the bl worktree so a subsequent rejection-and-rework picks up the squashed history cleanly.

The worktree and the branch stay intact. The worker's cwd is not destroyed — they can keep working in-place if the review is rejected.

### Close (reviewer approves)

`bl close bl-a1b2 -m "approved"` is the reviewer's approval step. Must run from the repo root (not from inside the bl worktree). It:

1. Removes the bl worktree and deletes `work/bl-a1b2`.
2. Archives the task on the state branch: records the closure in any parent's `closed_children` array, `git rm`s both the `.json` and the `.notes.jsonl`, and commits all of that as a single `state: close bl-a1b2 - title\n\n<reviewer message>` commit.
3. Removes the local claim file.

The task file is gone from the state branch's tree but preserved in its history — `git show balls/tasks~1:.balls/tasks/bl-a1b2.json` retrieves the last known state.

### Reject (reviewer requests rework)

```bash
# Set status back to in_progress. Agent resumes in existing worktree.
bl update bl-a1b2 status=in_progress --note "needs error handling"
# Agent's next `bl review` will merge main first, picking up this change.
```

---

## Conflict Resolution

The text-mergeable schema (sorted keys, one field per line) and the `merge=union` gitattribute on notes files push most concurrent edits into the "clean merge" category. `bl sync` only needs to run its custom resolver on the narrow case where two workers actually edited the same field of the same task.

### What merges cleanly under stock git

- **Different fields of the same task.** Sorted one-field-per-line layout means two workers editing `priority` vs `tags` produce non-overlapping diffs.
- **Different tasks.** One file per task; git never even sees them as related.
- **Concurrent notes.** `merge=union` on `*.notes.jsonl` appends both sides' lines.
- **Delete vs modify.** The resolver stages the surviving side (or `git rm`s when both sides deleted).

### Field-wise resolution (for real conflicts)

When two workers edit the same field of the same task, `bl sync` invokes the resolver:

1. **Status precedence:** `closed` > `review` > `in_progress` > `blocked` > `open` > `deferred`. Higher status wins.
2. **Notes:** Union by timestamp. Append-only, both sides' notes kept.
3. **Timestamps:** Later `updated_at` wins for all non-status fields.
4. **claimed_by:** If status resolves to `closed`, `claimed_by` comes from the closing side. Otherwise, first writer wins.

### Scenarios

**Same task claimed by two workers offline.** First push wins. Second worker's `bl sync` detects the divergence on the state branch, merges via the resolver, status stays `in_progress` under whichever worker committed first.

**Same task closed by two workers.** Both close commits land on the state branch. The second worker's sync sees the task already archived (missing from the tip's tree) and quietly moves on.

**One closes, one updates.** Closed wins. The update's notes are appended via `merge=union` and preserved.

**Different tasks edited concurrently.** No conflict. Different files, git merges cleanly.

---

## CLI Commands

### CLI look and feel

Read commands (`bl list`, `bl ready`, `bl show`, `bl dep tree`) share a visual language so you only learn it once.

- **Glyphs are anchors, not vocabulary.** Every glyph is paired with its word at the call site (`» in_progress`, `○ open`, `✓ closed`). You never have to memorize an icon.
- **Colors are additive, not load-bearing.** A run with color disabled — `--plain`, `NO_COLOR`, `CLICOLOR=0`, or output piped to a non-tty — renders the exact same words and structure with no information lost.
- **Priority dot.** Leading `●` (`*` in ASCII) colored red/yellow/blue/dim for priorities 1–4.
- **Status glyphs.** `○` open, `»` in_progress, `?` review, `⌀` blocked, `✓` closed, `~` deferred. ASCII fallbacks: `[ ]`, `[>]`, `[?]`, `[!]`, `[x]`, `[-]`.
- **Badges.** `★` claimed by you (`*` in ASCII), `◆` unmet deps (`D`), `⛓` open `gates` link (`G`).
- **Epic progress bar.** A 10-cell `██████░░░░ 6/10  60%` (`######---- 6/10  60%` in ASCII) appended to epic rows in `bl list` and shown as a `progress:` line in `bl show`.

#### Color and Unicode detection

Detection runs in this order:

1. `--plain` (any command) — force unstyled output: no color, no Unicode glyphs.
2. `NO_COLOR` env var present — disables color *and* Unicode glyphs (matches the [no-color.org](https://no-color.org) convention; users opting out of color usually also want stable ASCII).
3. `CLICOLOR=0` env var — disables color only; Unicode glyphs still render.
4. stdout `isatty()` — required for either color or Unicode. A piped `bl list | less` always renders ASCII.

**Machine contract.** `--json` output (`bl list --json`, `bl show --json`, `bl ready --json`, `bl dep tree --json`) is byte-identical to before the visual redesign. Scrapers and agents should always prefer `--json`.

### bl init [--stealth] [--tasks-dir PATH]

One-time setup per clone. `bl init` is idempotent and self-healing — running it on an already-initialized repo verifies and repairs. Specifically:

1. Creates `.balls/local/`, `.balls/plugins/`, `.balls/config.json` and adds the gitignore entries.
2. Creates or fetches the `balls/tasks` orphan branch. If the branch exists on `origin`, it's tracked; otherwise a fresh orphan is created and pushed (best-effort) so subsequent clones discover it.
3. Checks the state branch out as a second git worktree at `.balls/worktree/`.
4. Seeds `.balls/tasks/.gitattributes` with `*.notes.jsonl merge=union` on the state branch.
5. Creates the `.balls/tasks → worktree/.balls/tasks` symlink in main's working tree.
6. Commits the main-side additions (`.gitignore`, `config.json`, `plugins/.gitkeep`) as a single `balls: initialize` commit.

With `--stealth`, tasks are stored outside the repo at `~/.local/share/balls/<repo-hash>/tasks/` with no state branch at all. Useful for local-only planning that shouldn't appear in any git history. All other bl commands work identically; the orphan-branch topology is simply bypassed.

With `--tasks-dir PATH`, tasks are stored at the given absolute path instead of the auto-generated hash-based location. Implies `--stealth`. Useful for project integrations where multiple repos or external tools need tasks at a predictable, shared location (e.g. `bl init --tasks-dir /opt/project/tasks`).

**No-git mode:** `bl init --tasks-dir PATH` also works outside a git repository. In this mode balls stores tasks as flat JSON files at the given path with no git operations at all — no state branch, no commits, no worktrees. All commands work: `create`, `list`, `show`, `update`, `sync` (plugin-only), `ready`, `repair`. The only behavioral difference is that `bl claim` requires `--no-worktree` (since there's no git repo to create a worktree in), and `bl review`/`bl close` are status flips with no merge.

**By hand:** see `SPEC-orphan-branch-state.md` §11 for the full shell sequence (`git switch --orphan balls/tasks`, `git worktree add .balls/worktree balls/tasks`, `ln -s worktree/.balls/tasks .balls/tasks`, gitignore updates, initial commit).

**Bare hub:** everything above assumes a working tree. Plain `bl init` (no `--bare`) still cannot initialize a bare repo — there is no work tree to write the `balls: initialize` commit, the `.gitignore`, or the `.balls/tasks` symlink into, and that working-tree wiring is correctly *skipped* at a bare root, not faked. Standing up the recommended bare central-hub deployment is the dedicated `bl init --bare <source> <hubdir>` form (equivalently the by-hand sequence), documented in *The bare central hub → Bootstrapping a bare hub from scratch* above.

### bl create TITLE [options]

```
bl create "Implement auth middleware" -p 1 -t task --parent bl-x9y8 --dep bl-c3d4 --tag auth
```

Generates an ID, writes the task file into the state worktree, commits it on `balls/tasks`. Rejects circular deps and nonexistent dep IDs. Triggers plugin push if configured.

**By hand:**
```bash
$EDITOR .balls/tasks/bl-NEW.json           # write the JSON directly through the symlink
git -C .balls/worktree add .balls/tasks/bl-NEW.json
git -C .balls/worktree commit -m "balls: create bl-NEW"
```

### bl list [filters]

```
bl list                    # all non-closed
bl list --status open      # only open
bl list -p 1               # only priority 1
bl list --parent bl-x9y8   # children of a parent
bl list --tag auth         # by tag
bl list --closed           # only closed, reconstructed from history
bl list --all              # open and closed together
```

Without a status filter, `bl list` groups tasks by status under one-line headers and nests in-group children under their parent. With `--status X`, output is flat but the status column stays so the visual language matches.

Closed tasks are `git rm`'d from the `balls/tasks` state branch, so `--closed` (alias: `--status closed`) and `--all` walk that branch's deletion history to reconstruct them — high-volume on a long-lived repo, and rendered flat since the grouped view is for live work. Recovery needs the state branch: a stealth/no-git store prints an "unavailable" note and lists nothing closed.

Sample output (in a real terminal: priority dot is colored, status word is colored, glyphs render as Unicode):

```
[>] in_progress
● [>] in_progress  bl-25db   Swap auth middleware                          api, auth

[ ] open
● [ ] open         bl-a847 G CLI display overhaul  [epic]  ██████░░░░ 6/10  60%
● [ ] open           bl-21a5 ★ ready redesign                              display
● [ ] open           bl-adaf D show redesign                               display
```

Each row is `prio_dot status_glyph status_word  id badges title  tags`. Badges: `★` claimed by you, `D` (`◆` in Unicode) unmet deps, `G` (`⛓`) open `gates` link. Epic rows append a 10-cell progress bar and percentage. Children indent under the parent within the same status group; if a child's parent is in a different group, the child renders as a root in its own group.

**By hand:** `for f in .balls/tasks/bl-*.json; do jq '.' "$f"; done | jq -s 'sort_by(.priority, .created_at)'`

### bl ready

```
bl ready                   # list ready tasks
bl ready --json            # machine-readable
```

Computes the ready queue. Auto-fetches if local state is older than `stale_threshold_seconds` from config (default 60s). `--no-fetch` to skip.

Output format mirrors `bl list`'s flat mode (priority dot + status column + id + badges + title + tags) and appends a dim `↑ bl-xxxx (parent title)` hint whenever the task has a parent, so an agent picking work doesn't lose the surrounding epic.

**By hand:** List open tasks, filter to those with all deps closed and no `claimed_by`, sort by priority.

### bl show ID

```
bl show bl-a1b2
bl show bl-a1b2 --json
bl show bl-a1b2 --verbose
bl show bl-a1b2 --resolve-remote
```

Lays out a styled header (priority dot + status glyph + id + title + claimed badge), a metadata row (`type`, `created`, `updated` — relative timestamps; `--verbose` appends absolute ISO), an optional `tags:` line, an optional `progress:` row for epics, a relations block (deps with inline statuses, gates, parent + parent title, children, delivered, branch, remote, dep_blocked when relevant), a wrapped description, and an oldest-first notes log.

The delivery line looks like `delivered: e69193f Add bl completions... [bl-1a34]`; if the cached `delivered_in` SHA is stale, the tag scan on main still finds the commit and the display is annotated `(hint stale)`. `--json` exposes `delivered_in_resolved`, `delivered_in_hint_stale`, and (for `type=epic` tasks) a `progress: { closed, total }` object alongside the task.

`--resolve-remote` opts into cross-repo delivery resolution: on a local miss it fetches the task's `delivered_repo` into a balls-owned code-refs cache and re-runs the tag scan, so a task delivered from a *different* clone still resolves its `delivered:` line. Off by default — fetching from arbitrary forge URLs is rude without the operator asking for it.

A **closed** task's id still resolves: when it's no longer on the state-branch HEAD, `bl show` reconstructs it from the `balls/tasks` deletion history (status overlaid as `closed`, `closed_at` taken from the close commit), then renders and resolves its `delivered:` line exactly as for a live task. No flag is needed — the not-found path falls back automatically.

If a plugin has populated `task.external.<plugin>` with `remote_key` and/or `remote_url` (the Plugin Protocol convention — see below), `bl show` surfaces them as a `remote:` block so agents don't have to parse `--json` to find a Jira key or issue URL. Plugins whose blob has neither field are skipped — the human view doesn't dump arbitrary plugin internals.

**By hand:** `cat .balls/tasks/bl-a1b2.json | jq .` — the symlink transparently reads from the state worktree.

### bl claim ID [--as IDENTITY] [--no-worktree]

```
bl claim bl-a1b2
bl claim bl-a1b2 --as dev1/agent-alpha
bl claim bl-a1b2 --no-worktree
```

Validates the task is claimable → flips status/claimed_by/branch on the state branch → commits (`balls: claim bl-a1b2`) → creates a git worktree at `.balls-worktrees/bl-a1b2/` on `work/bl-a1b2` → symlinks `.balls/local`, `.balls/worktree`, and `.balls/tasks` into the new worktree → writes the local claim file → prints the worktree path. Triggers plugin push if configured.

With `--no-worktree`, skips worktree creation — only flips the task status and writes the claim file. Required in no-git mode; optional in git mode for workflows that don't need branch isolation.

Fails if already claimed locally, deps unmet, or task not `open`.

### bl review ID [-m MSG]...

```
bl review bl-a1b2 \
  -m "Short title under ~50 chars" \
  -m "Body paragraph explaining the change in detail. Wrap at ~72." \
  -m "Add another -m for another paragraph."
```

Worker's exit point. Commits uncommitted work in the bl worktree → merges main in (surfaces conflicts there, not on main) → squash-merges to main as a single feature commit → writes the `delivered_in` hint and flips the task to `review` on the state branch in one commit. The worktree and the claim stay intact so a rejected review can be reworked in place.

Commit messages use 50/72 shape. `-m` is repeatable, exactly like `git commit -m … -m …`: the first `-m` is the title (with `[bl-xxxx]` appended), each later `-m` is a body paragraph separated by a blank line. A single `-m` value may itself span multiple lines (first line = title, rest = body), so `-m "$(cat <<'EOF' … EOF)"` also works. A single-line `-m "fix foo"` still works (no body). Don't stuff a multi-sentence summary into a single line — `git log --oneline` becomes unreadable.

If the reviewer rejects (`bl update bl-a1b2 status=in_progress`), the worker resumes in the existing bl worktree and calls `bl review` again; the next run merges main first, picking up the rejection.

### bl close ID [-m MSG]...

```
bl close bl-a1b2 -m "approved"
```

Reviewer approval. Removes the bl worktree, deletes `work/bl-a1b2`, and archives the task on the state branch (parent bookkeeping, `git rm` of `.json` and `.notes.jsonl`, and the `state: close` commit in one atomic locked sequence). **Rejects if run from inside the worktree** — must run from the repo root, which `bl close` prints on success so you can `cd` back. On a bare hub the repo root is the bare directory itself (there is no checked-out main to stand in); `bl close` runs there normally — see *The bare central hub*.

The reviewer message is embedded in the state-branch close commit's body (not appended to a notes file, which is about to be deleted). It's still in git history on `balls/tasks`. `-m` is repeatable here too — each value becomes its own paragraph.

Three flags control how `bl close` resolves the delivering commit, mostly in deferred mode or when closing a squash produced by another clone: `--delivered SHA` pins the commit instead of tag-scanning the target branch (useful when a forge rebase-merge left several commits); `--delivered-repo URL` overrides the recorded `delivered_repo` provenance when closing on behalf of another clone; `--resolve-remote` opts into fetching `delivered_repo` into the balls-owned code-refs cache and re-running the tag scan when the squash isn't on this clone's target branch (it auto-engages in deferred mode). See *Delivery Modes*. `--sync` / `--no-sync` toggle the state-branch round-trip — see the `require_remote_on_close` config row.

### bl update ID [field=value ...] [--note TEXT]

```
bl update bl-a1b2 priority=2
bl update bl-a1b2 status=blocked --note "Waiting on API team"
bl update bl-a1b2 status=closed        # closing unclaimed tasks skips the bl close path
```

Edits fields directly on the state branch (no bl worktree required) and commits `balls: update bl-a1b2 - title`. Notes are appended to the sibling `.notes.jsonl` file. `status=closed` on an unclaimed task goes through the same atomic archive as `bl close`.

**By hand:** see `SPEC-orphan-branch-state.md` §11 for the canonical edit-and-publish shell sequence (`$EDITOR .balls/tasks/bl-a1b2.json; git -C .balls/worktree add .balls/tasks/bl-a1b2.json; git -C .balls/worktree commit -m "bl-a1b2: bumped priority"`).

### bl drop ID [--force]

Releases a claim. Resets task file to open/unclaimed/no-branch, removes worktree, removes local claim, commits. `--force` required if worktree has uncommitted changes (they are lost).

**By hand:** Edit task JSON, `git worktree remove`, `rm` claim file, commit.

### bl dep add TASK DEPENDS_ON

Appends to `depends_on`. Rejects cycles. Commits.

### bl dep rm TASK DEPENDS_ON

Removes from `depends_on`. Commits.

### bl dep tree [ID]

Walks the parent/child hierarchy and prints it as a real tree using box-drawing characters (Unicode `├─ │  └─`, ASCII `|- |  ` `` `- `` fallback). Dep edges and gates are shown as inline annotations on each row, never as nesting. Without ID, every parentless task renders as its own top-level tree. `--json` emits a nested `{id, title, status, hier_path, children}` shape (`hier_path` omitted for roots).

Each non-root row carries a dotted sibling-position annotation next to its id (`.1`, `.1.2`, …) so a reader can see the parent chain without cross-referencing `parent`. The annotation is pure display — ids themselves are still the flat `bl-<hex>` form used everywhere else.

```
bl-a3f8  Migrate auth layer  [epic]  ○ open
├── bl-1234 .1  Extract token store                  ✓ closed
├── bl-5678 .2  Swap middleware                      » in_progress
│   └── bl-9abc .2.1  Audit rollback plan            ⛓ gates parent
└── bl-def0 .3  Cut over prod switch                 ⌀ blocked by bl-5678
```

Cycles in parent edges (which shouldn't happen in healthy data) are detected and marked with `↺ cycle` so the renderer doesn't loop.

### bl link add TASK TYPE TARGET

```
bl link add bl-a1b2 relates_to bl-c3d4
bl link add bl-a1b2 duplicates bl-e5f6
bl link add bl-a1b2 supersedes bl-g7h8
bl link add bl-a1b2 replies_to bl-i9j0
bl link add bl-a1b2 gates     bl-k1l2
```

Adds a typed link. Link types: `relates_to`, `duplicates`, `supersedes`, `replies_to`, `gates`. Validates target exists. Idempotent. Commits. See [Gates: post-review blockers](#gates-post-review-blockers) for what `gates` does.

### bl link rm TASK TYPE TARGET

Removes a typed link. Commits.

## Gates: post-review blockers

Gates are the answer to a question every shipping team eventually asks: *when the implementation is done, how do I make sure the security review, the doc update, and the test-coverage audit actually happen before the task is archived?*

Most trackers handle this with process — a checklist in the ticket, a reminder, a Slack ping, a hope. Balls makes it a first-class link type.

A `gates` link says: *this parent task cannot transition to `closed` until the target task is closed first.* It's structurally different from a `dep`:

| | `dep` (depends_on) | `gates` |
|---|---|---|
| Blocks | **claim** of the child | **close** of the parent |
| Direction | child → parent (child blocks on parent finishing) | parent → child (parent blocks on child finishing) |
| Typical use | "build the API before the UI that consumes it" | "security audit before the feature ships" |

### Worked example

You just finished implementing a new auth middleware. Code is in review. Before it ships, you want three audits: security, docs, test coverage. Here's the whole flow:

```
# Create the audit children.
bl create "Security audit: auth middleware" --parent bl-auth
bl create "Doc review: auth middleware"     --parent bl-auth
bl create "Test coverage: auth middleware"  --parent bl-auth
# (Say these come back as bl-sec, bl-doc, bl-cov.)

# Wire them as gates on the parent.
bl link add bl-auth gates bl-sec
bl link add bl-auth gates bl-doc
bl link add bl-auth gates bl-cov

# Now try to close the parent too early.
bl close bl-auth
# Error: cannot close bl-auth: blocked by open gates bl-sec, bl-doc, bl-cov.
#        Close the gate tasks first, or run `bl link rm bl-auth gates <id>` to drop a gate.

# Finish the audits one by one; when the last one closes, the parent closes cleanly.
```

### Why it's a primitive, not a convention

A checklist in a description is a convention: nothing enforces it, and it rots. A gate is a data-structure-level invariant — `close_and_archive` literally refuses to run while any gate child is still open. You can't bypass it with a typo or a hurry, only by explicitly dropping the gate link, which leaves a commit trail.

It's also additive. Existing projects get nothing new to learn until they want gates; existing tasks keep working unchanged. And because `gates` is just another link-type variant in the same JSON schema, older `bl` binaries that predate this feature still round-trip the link verbatim — the worst they can do is fail to *enforce* the gate, not corrupt the task file. (That forward-compat guarantee kicks in starting with this release; `bl` versions before `0.3.0` will hard-error on a `gates` link, which is why `0.3.0` is a breaking version bump.)

### When to reach for gates

- Post-implementation audits (security, docs, test coverage, accessibility, perf).
- Cross-team sign-offs that need to happen *after* code is merged but *before* the task closes.
- Any "one task, many mandatory follow-ups" pattern where forgetting one is expensive.

### When *not* to

- Pre-implementation blockers — use `dep`. Gates is about close, not claim.
- Soft recommendations — gates is a hard stop. If "we should probably also do X" is fine, it's not a gate.

### bl sync

```
bl sync
```

Reconciles both main and the state branch with `origin`:

1. `git fetch origin` (best-effort; offline is fine).
2. **State branch first.** In `.balls/worktree/`, merge `origin/balls/tasks`, auto-resolve any task-file conflicts via the field-wise resolver, push `balls/tasks`.
3. **Main second.** In main, merge `origin/main`, push.
4. **Half-push detection.** Scan the state branch for `state: close bl-xxxx` commits whose corresponding `[bl-xxxx]` tag is not reachable from main, and surface them as warnings. A half-push happens if the state push succeeded but the main push failed on a previous invocation — next sync naturally retries main, but the warning tells you explicitly if the local repo can't heal it (e.g., on a different machine).
5. Run plugin sync (if configured). Plugin output is bounded and timed (see Plugin System).

Push ordering matters: state branch goes first so that if the sync is interrupted between pushes, the closing commit is already visible to other workers — they'll see the task as closed even though the feature commit is still coming.

**By hand:** see `SPEC-orphan-branch-state.md` §11. The shell sequence is two `git -C .balls/worktree push origin balls/tasks` plus a `git push origin main`, with `git fetch` and `git merge` between as needed.

#### Human-gate review (`--review`, `--apply`, `--discard`)

Plugin sync reports normally apply immediately; that is fine when the operator trusts the plugin to do the right thing on every push. When you want a chance to look first, run `bl sync --review`: each plugin's `SyncReport` is written to `.balls/local/pending-sync/sync/<id>.json` instead of being applied. State-branch sync is suppressed in this mode — the gate is a pre-apply hold, not a remote round-trip.

```
bl sync --review                 # stage; nothing committed
bl sync --list-staged            # one line per pending entry
bl sync --apply <id>             # replay the staged report through the normal path
bl sync --discard <id>           # drop the staged file, no commit
```

Staged files are local-only and gitignored under `.balls/local/`. They survive across invocations until you apply or discard them. Applying re-uses the same `apply_sync_report` path as live sync, so per-item warnings (unknown task ids, malformed updates) behave identically.

### bl prime [--as IDENTITY]

Session bootstrap for agents. Runs `bl sync`, then outputs:
- Worker identity
- Ready tasks ranked by priority
- Currently claimed tasks for this identity (for session resume)

Designed to be injected into an agent's context at session start.

### bl resolve FILE

Manual conflict resolution helper: parses both sides of a conflicted task file, applies the field-wise resolution rules, writes the result. Rarely needed in the new topology — most conflicts merge cleanly under stock git — but available for edge cases.

### bl doctor

Read-only diagnostic for repo/bl state drift. The complaint it answers: an `AGENTS.md` references bl in a repo that was never `bl init`'d, or the store *is* there but has drifted, and today both only surface as an opaque error part-way through a workflow. `doctor` turns that into an up-front, specific message and the command that fixes it.

It changes nothing — `repair` remains the only action verb; doctor only diagnoses and suggests. Exit is always 0; the verdict is the text. Checks:

- discovery fails — surfaces the precise reason (wrong directory, untracked repo, broken state worktree, …), and when no `.balls/` exists at all but a doc references bl, says so explicitly: run `bl init` or scrub the docs;
- `.balls/config.json` unreadable;
- `.balls/local/tasks_dir` override pointing at a missing path;
- the state worktree present but not a valid linked git worktree;
- stale claim files (no such task, or the task isn't in progress);
- worktree dirs under `.balls-worktrees/` with no task or claim behind them.

Run it when bl misbehaves, or before trusting an unfamiliar repo. Healthy repos print one line and exit.

---

## Config (.balls/config.json)

Committed to main, shared across the team.

```json
{
  "version": 1,
  "id_length": 4,
  "stale_threshold_seconds": 60,
  "auto_fetch_on_ready": true,
  "require_remote_on_claim": false,
  "require_remote_on_review": false,
  "require_remote_on_close": false,
  "worktree_dir": ".balls-worktrees",
  "delivery": { "mode": "local-squash" },
  "review": { "pre_check": null },
  "target_branch": null,
  "min_bl_version": null,
  "plugins": {
    "jira": {
      "enabled": true,
      "sync_on_change": true,
      "config_file": ".balls/plugins/jira.json"
    }
  }
}
```

| Field | Description |
|---|---|
| `version` | Config schema version. Currently `1`. |
| `id_length` | Hex chars in generated task IDs. Clamped to `[4, 32]` on load; out-of-range values produce a warning and fall back to 4. |
| `stale_threshold_seconds` | `bl ready` auto-fetches if the last fetch is older than this. |
| `auto_fetch_on_ready` | Whether `bl ready` auto-fetches at all. |
| `require_remote_on_claim` | When true, `bl claim` round-trips the claim commit through `origin/balls/tasks` before creating the worktree. Closes the offline-agent claim race; off by default. Per-clone override: `.balls/local/config.json` (gitignored) with `{"require_remote_on_claim": false}`. Per-invocation override: `bl claim --sync` / `--no-sync`. The remote is reachability-probed up front; if unreachable, the claim fails loudly rather than silently falling back to local-only. |
| `require_remote_on_review` | When true, `bl review` pushes the state-branch review commit to `origin/balls/tasks` before the transition is considered complete. A required-policy failure rolls back the squash on main and the state-branch commit so the task stays observably in `in_progress`. Same precedence chain as `require_remote_on_claim`; per-invocation override `bl review --sync` / `--no-sync`. |
| `require_remote_on_close` | When true, `bl close` pushes the state-branch archive commit to `origin/balls/tasks` before the worktree is torn down. A required-policy failure leaves the worktree, claim file, and task file in place for retry. Same precedence chain as the others; per-invocation override `bl close --sync` / `--no-sync`. |
| `worktree_dir` | Where `bl claim` creates worktrees. Must be a relative path under the repo; values containing `..` or starting with `/` are rejected on load. |
| `tasks_dir` | *(removed in 0.3.4)* Stealth-mode task storage is controlled via `bl init --stealth [--tasks-dir PATH]` and persisted in `.balls/local/tasks_dir`, not in the committed config. Older configs that carry this field are unaffected — it was never read. |
| `delivery.mode` | `"local-squash"` (default) or `"deferred"`. Selects the `bl review` code path — see *Delivery Modes*. An absent `delivery` block equals `{"mode": "local-squash"}`; the default is bit-identical to every prior version. |
| `review.pre_check` | Shell command `bl review` runs in the worktree after the integration branch is merged in and *before* the squash; a non-zero exit aborts the review (no squash, no push, no status flip), so the project's quality gate runs at the merge, not just in CI. `null` (or an absent `review` block — the default) ⇒ no gate. See *Delivery Modes → Pre-squash review gate*. |
| `target_branch` | Repo-level integration branch. `null` (default) falls back to the branch checked out at the repo root — the historical, previously-undocumented behavior. A per-task `target_branch` field overrides this (e.g. a hotfix targeting `main` on a `develop`-default repo). Required (non-null) when `delivery.mode = "deferred"`. |
| `min_bl_version` | Advisory only. Newer `bl` clients warn when their version is below this; older clients ignore it. Surfaces the deferred-mode caveat (an old client local-squashes instead of deferring) without engineering prevention. |
| `plugins` | Per-plugin enable/sync flags and config file paths. |

### Environment overrides

| Variable | Purpose | Default |
|---|---|---|
| `BALLS_IDENTITY` | Worker identity for claims and notes | `$USER`, then `"unknown"` |
| `BALLS_PLUGIN_TIMEOUT_SECS` | Wall-clock cap on any plugin invocation | 30 |
| `BALLS_PLUGIN_MAX_STREAM_BYTES` | Max bytes buffered from a plugin's stdout/stderr | 1 MiB |
| `BALLS_PLUGIN_ABS_MAX_STREAM_BYTES` | Absolute hard ceiling on bytes buffered from a plugin stream. Independent of (and never lifted by) a raised `BALLS_PLUGIN_MAX_STREAM_BYTES`, so loosening the stream cap for a large sync can't disable memory protection. Far above any real payload — only a runaway/abusive plugin hits it. | 64 MiB |
| `BALLS_PLUGIN_MAX_SYNC_CREATES` | Flood backstop: max tasks created from one plugin sync. Excess is skipped with a warning, the rest of the sync still applies. Set in the thousands; a real tracker never reports this many new issues at once. | 5000 |
| `BALLS_PLUGIN_MAX_SYNC_FIELD_BYTES` | Per-text-field byte ceiling on a synced title/description/note. An oversize field is truncated with a visible marker (never dropped, never rejected); siblings and the rest of the sync are unaffected. Absurdly generous — real fields are bytes to kilobytes. | 1 MiB |

---

## Plugin System

### Design

Plugins are external executables that implement a defined interface. Core knows how to call them but never contains integration-specific logic. Auth flows (Single Sign-On (SSO), Personal Access Tokens (PATs), OAuth, etc.) are entirely the plugin's responsibility, managed in `.balls/local/plugins/` where credentials and tokens live (gitignored, never committed).

### Interface

A plugin is an executable (any language) that responds to commands via argv:

```
balls-plugin-jira auth-setup --config .balls/plugins/jira.json --auth-dir .balls/local/plugins/jira/
balls-plugin-jira auth-check --config .balls/plugins/jira.json --auth-dir .balls/local/plugins/jira/
balls-plugin-jira push --task bl-a1b2 --config .balls/plugins/jira.json --auth-dir .balls/local/plugins/jira/
balls-plugin-jira sync --config .balls/plugins/jira.json --auth-dir .balls/local/plugins/jira/
balls-plugin-jira sync --task bl-a1b2 --config .balls/plugins/jira.json --auth-dir .balls/local/plugins/jira/
```

### Commands a plugin must implement

| Command | Input | Output | Description |
|---|---|---|---|
| `auth-setup` | Reads `config`, writes creds to `auth-dir` | (interactive) | One-time auth configuration. Handles SSO, PAT entry, OAuth flows — whatever the service needs. The config is passed so plugins that target multiple instances know which one to authenticate against. |
| `auth-check` | Reads `config` and `auth-dir` | Exit 0 if valid, 1 if expired/missing | Tests whether current credentials work. Core calls this before push/sync. |
| `push --task ID` | Task JSON on stdin, config, auth | JSON on stdout (see Push Response Schema) | Pushes one task's state to the remote tracker. Returns external metadata for core to store. |
| `sync [--task ID]` | All tasks JSON on stdin, config, auth | JSON on stdout (see Sync Report Schema) | Bidirectional sync. Optional `--task` filters to a single item by local ball ID or remote key. |

### Push response schema

After a successful push (exit 0), the plugin writes a JSON object to stdout. Core stores this object verbatim into `task.external.{plugin_name}`, overwriting any previous value. The plugin decides what fields to include. At minimum, include `remote_key` so the task can be correlated with the remote issue.

```json
{
  "remote_key": "PROJ-123",
  "remote_url": "https://company.atlassian.net/browse/PROJ-123",
  "synced_at": "2026-04-10T12:00:00Z"
}
```

All fields are plugin-defined. Core treats this as an opaque `serde_json::Value`. Empty stdout or `{}` means "no external metadata to store" (valid for notification-only plugins like Slack).

If the task's `external.{plugin_name}` already contains a `remote_key`, this is an update. If not, this is a create. The plugin inspects the incoming task JSON to determine which.

### Sync report schema

After a successful sync (exit 0), the plugin writes a JSON object to stdout describing what changed. Core processes each section:

```json
{
  "created": [
    {
      "title": "New issue from Jira",
      "type": "task",
      "priority": 2,
      "status": "open",
      "description": "Created in Jira by someone else",
      "tags": ["imported"],
      "external": {
        "remote_key": "PROJ-456",
        "remote_url": "https://company.atlassian.net/browse/PROJ-456",
        "synced_at": "2026-04-10T12:00:00Z"
      }
    }
  ],
  "updated": [
    {
      "task_id": "bl-a1b2",
      "fields": {
        "status": "in_progress",
        "priority": 1
      },
      "external": {
        "remote_key": "PROJ-123",
        "synced_at": "2026-04-10T12:00:00Z"
      },
      "add_note": "Status changed to In Progress in Jira by user@example.com"
    }
  ],
  "deleted": [
    {
      "task_id": "bl-c3d4",
      "reason": "Issue PROJ-789 deleted in Jira"
    }
  ]
}
```

All three arrays are optional. Empty arrays or omitted arrays mean nothing changed in that category. An empty object `{}` is valid.

**`created` entries** — remote-only issues the plugin wants core to create locally:

| Field | Required | Default | Description |
|---|---|---|---|
| `title` | yes | — | Task title |
| `type` | no | `"task"` | Free-form identifier; e.g. `task`, `bug`, `epic`, `feature`, `spike` |
| `priority` | no | `3` | 1 (highest) to 4 (lowest) |
| `status` | no | `"open"` | One of: `open`, `in_progress`, `blocked`, `closed`, `deferred` |
| `description` | no | `""` | Full description |
| `tags` | no | `[]` | Array of tag strings |
| `external` | no | `{}` | Stored into `task.external.{plugin_name}`. Should contain at least `remote_key`. |

**`updated` entries** — existing local tasks with remote changes:

| Field | Required | Default | Description |
|---|---|---|---|
| `task_id` | yes | — | The ball task ID (e.g., `"bl-a1b2"`) |
| `fields` | no | `{}` | Partial object. Accepted keys: `title`, `priority`, `status`, `description`. Unknown keys are silently ignored. |
| `external` | no | `{}` | Replaces `task.external.{plugin_name}` |
| `add_note` | no | — | If present, appended as a note attributed to the plugin name |

**`deleted` entries** — remote issues that no longer exist:

| Field | Required | Default | Description |
|---|---|---|---|
| `task_id` | yes | — | The ball task ID |
| `reason` | no | `"Deleted in remote tracker"` | Explanation appended as a note |

Core sets the task status to `deferred` and appends the reason as a note. Tasks already `closed` are skipped. The task file is not deleted.

### Ingest backstops

Bidirectional sync makes every title, description, tag, note, and `external` blob in a sync report attacker-influenced — and each lands as a committed file on `balls/tasks`. Core does **not** police content: big titles, long descriptions, many tags, and fat `external` maps are all legitimate and pass through byte-unchanged. The only thing guarded against is pathological abuse that would OOM the process or wedge the repo, and every guard is a generous backstop set far above any plausible real payload, env-overridable, and warn-not-fail:

- **Whole-stream memory** — a plugin's stdout is bounded by `BALLS_PLUGIN_MAX_STREAM_BYTES` (1 MiB) *and* an absolute `BALLS_PLUGIN_ABS_MAX_STREAM_BYTES` backstop (64 MiB) that a raised stream cap can never lift. Over the effective cap, the report is discarded with a warning naming the knob.
- **Oversized field** — a title/description/note past `BALLS_PLUGIN_MAX_SYNC_FIELD_BYTES` (1 MiB) is truncated with a visible `[…balls truncated this field …]` marker and a diagnostic. The field's siblings and the rest of the sync still apply; nothing is rejected. `external` slices are *not* size-policed individually — they ride the whole-stream backstop only.
- **Create flood** — more than `BALLS_PLUGIN_MAX_SYNC_CREATES` (5000) creates in one sync is treated as a flood: the excess is skipped with a warning and the rest applies. A real tracker never reports thousands of new issues at once; if yours legitimately does, raise the knob.

If any of these ever bites a real repo, the limit is too tight — raise the corresponding environment variable (see [Environment overrides](#environment-overrides)).

### Sync stdin

When core calls `sync`, it sends all local tasks as a JSON array on stdin (same format as `bl list --json --all`). The plugin uses this to determine which local tasks need pushing and which remote tasks are new.

When `--task ID` is passed, the plugin should filter its operations to the specified item. The ID may be a local ball ID (e.g., `bl-a1b2`) or a remote key (e.g., `PROJ-123`) — the plugin is responsible for resolving which.

### Plugin config (.balls/plugins/jira.json)

Git-tracked. Contains non-secret configuration.

```json
{
  "url": "https://company.atlassian.net",
  "project": "PROJ",
  "status_map": {
    "open": "To Do",
    "in_progress": "In Progress",
    "blocked": "Blocked",
    "closed": "Done",
    "deferred": "Backlog"
  },
  "field_map": {
    "priority": "priority",
    "description": "description",
    "tags": "labels"
  },
  "sync_filter": "project = PROJ AND status != Done",
  "create_in_remote": true,
  "close_in_remote": true
}
```

### Plugin auth (.balls/local/plugins/jira/)

Gitignored. Plugin owns this directory entirely. Might contain:

```
.balls/local/plugins/jira/
├── token.json           # OAuth tokens, PATs, session cookies
├── .sso-cache           # SSO session state
└── auth-meta.json       # token expiry, refresh timestamps
```

Core never reads these files. Core only passes the directory path to the plugin.

### Diagnostics channel (`BALLS_DIAG_FD`)

Plugins have stdout (for the JSON protocol) and stderr (unstructured text balls prints verbatim). For user-facing diagnostics that deserve structure — error codes, hints, the task id the problem applies to — balls also opens a dedicated diagnostics channel and advertises its fd via the `BALLS_DIAG_FD` environment variable.

A plugin that ignores this env var is unaffected: the channel is a silent no-op. A plugin that wants to use it writes newline-delimited JSON records to the fd — one object per line — and balls parses each record and renders it on stderr.

```sh
# inside a plugin (POSIX sh example)
if [ -n "$BALLS_DIAG_FD" ]; then
    printf '%s\n' '{"level":"error","code":"AUTH_EXPIRED","message":"token expired 2026-04-10","hint":"run auth-setup","task_id":"bl-abcd"}' >&"$BALLS_DIAG_FD"
fi
```

Record schema:

| Field | Required | Description |
|---|---|---|
| `level` | yes | `error`, `warning`, or `info` (rendered verbatim) |
| `message` | yes | Human-readable summary |
| `code` | no | Stable machine-readable identifier, shown in brackets |
| `hint` | no | Suggested remediation, rendered on its own line |
| `task_id` | no | Local ball id the diagnostic applies to, if any |

Malformed lines produce a single warning and do not abort the rest of the stream. The channel is available on every subcommand (`auth-setup`, `auth-check`, `push`, `sync`) and is subject to the same stream-size cap as stdout/stderr.

### Native plugin protocol (describe / propose)

The push/sync interface above is the *legacy* protocol: a plugin announces nothing about itself, returns a single opaque blob per task, and can't signal conflicts, decline a transition, or observe `create`/`drop`. A plugin opts into the **native protocol** — the participant-model wire (`SPEC-lifecycle-sync-participants.md` §5) — by shipping two extra subcommands: `describe` and `propose`. Both are independent additions; legacy `push`/`sync`/`auth-*` keep working unchanged, and a plugin that doesn't implement `describe` is silently shimmed onto the legacy path (SPEC §12). The two protocols are not stacked: a plugin that ships `describe` is driven through native `propose` and the legacy `push` is no longer called for the subscribed events.

The native protocol is what makes a plugin a real *participant*: it gets to declare what subset of `Task` it owns, return a structured conflict report that the negotiation primitive merges and retries (SPEC §7–§8), veto a transition outright (`reject`, §8.1), and observe `create`/`drop`. None of this is reachable from `push`/`sync`.

#### `describe`: self-registration

```
balls-plugin-jira describe --config .balls/plugins/jira.json --auth-dir .balls/local/plugins/jira/
```

Reads no stdin; writes a single JSON object to stdout (exit 0 on success). The response is parsed leniently per SPEC §13: unknown event names in `subscriptions` are dropped from the resolved set, the rest still take effect, and unknown top-level keys are ignored.

```json
{
  "subscriptions": ["claim", "review", "close", "update", "create", "sync"],
  "projection": {
    "owns": ["external"],
    "reads": ["id", "title", "status"],
    "external_prefixes": ["jira"]
  },
  "retry_budget": 5,
  "wants_context": false
}
```

| Field | Required | Description |
|---|---|---|
| `subscriptions` | yes | Events the plugin participates in (`claim`, `review`, `close`, `update`, `create`, `drop`, `sync`). Per-event semantics below. Unknown event strings are dropped silently per SPEC §13. |
| `projection.owns` | yes | Canonical `Task` field names this plugin authoritatively owns. Overlapping `owns` between two participants is a config-validation error (SPEC §5). Most plugins own only `external`. |
| `projection.reads` | no | Canonical fields the plugin reads but does not own. Used by the merge composer to reason about disjointness; informational, not enforced. |
| `projection.external_prefixes` | no | Prefixes within `task.external` this plugin owns (e.g. `["jira"]` ⇒ owns `task.external.jira.*`). Lets two plugins co-own `external` without colliding. |
| `retry_budget` | no | Override for the negotiation retry cap on a recoverable `conflict`. Defaults to 5 (SPEC §7). |
| `wants_context` | no | If true, every `propose` invocation receives `--ctx-file PATH` carrying an `EventCtx` document (§5.1, schema below). Absent/false ⇒ byte-identical stdin to today; no side channel. |

#### `propose`: per-event negotiation

For each (event, task) the plugin is subscribed to, core calls:

```
balls-plugin-jira propose --event claim [--ctx-file /tmp/balls-ctx-NNNN.json] \
    --config .balls/plugins/jira.json --auth-dir .balls/local/plugins/jira/
```

Stdin: the post-image `Task` as JSON (same shape as `bl show --json`'s `task`). The `--event` flag names which lifecycle event is firing; the plugin uses it to branch its behavior. `--ctx-file` is present **only** when the plugin declared `wants_context: true`; the path is to a temp file balls writes before spawn and removes after the child exits.

Stdout: a single JSON object with at most one of `ok`, `conflict`, or `reject` populated. Empty stdout (exit 0) or a response with none of the three set is treated as `Other` — wire failure, not a successful proposal. Unknown top-level keys are captured (not dropped) per SPEC §13 and degrade to `Other` with the variant named in the diagnostic.

**`ok` — successful proposal.** The plugin returns the projection of `Task` it owns; balls folds those fields into the working task and continues the event.

```json
{
  "ok": {
    "task": {
      "external": { "jira": { "remote_key": "PROJ-123", "synced_at": "2026-05-19T12:00:00Z" } }
    },
    "commit_policy": { "kind": "commit", "message": "mirror to PROJ-123" }
  }
}
```

`commit_policy` is optional and follows SPEC §10. Variants: `{"kind": "commit", "message": "..."}` (default; participant-supplied message is wrapped with a `plugin: <name>: ` prefix on the title), `{"kind": "batch", "tag": "..."}` (coalesce with other participants returning the same tag within the same event), or `{"kind": "suppress"}` (apply state, no commit — disallowed for required participants and rejected at apply time).

**`conflict` — recoverable field clash.** The plugin saw a remote-side change since its last sync that invalidates this proposal. Balls folds `remote_view` into the working task and re-invokes `propose` up to `retry_budget` times. Legacy `push`-shim plugins cannot emit `conflict` — only native participants get the retry-on-conflict path (SPEC §8).

```json
{
  "conflict": {
    "fields": ["status", "external.jira.assignee"],
    "remote_view": {
      "status": "in_progress",
      "external": { "jira": { "assignee": "bob" } }
    },
    "hint": "ticket was reassigned to bob in Jira since last sync"
  }
}
```

**`reject` — deliberate policy veto.** The plugin refuses the transition for a reason it states. *No* Task state, *no* retry, *no* merge. The failure policy (SPEC §9, configured per subscription in `.balls/config.json` — see *Participant enforcement* below) decides what the lifecycle event does about it:

```json
{ "reject": { "reason": "CI is red on this branch; close blocked" } }
```

- `required` ⇒ the event aborts; the Task is rolled back to its pre-event state; the plugin name and `reason` propagate verbatim to the caller.
- `best-effort` ⇒ warn and continue; the event ships; the rejection is recorded in `task.sync_status.<plugin>`.
- `gating` ⇒ stage for `bl sync --review`; the event proceeds in a pending-external state.

A per-invocation override (`--skip=<plugin>`, or `--no-sync` for the git-remote participant) overrides a required `reject` and is logged in the state-branch commit subject (SPEC §11) — soft policy, hard primitives. A `reject` is **not** the same as a `conflict` (recoverable, retried) or a wire failure (`Other`); conflating them is a regression.

#### Per-event semantics

The events a plugin can subscribe to via `describe`:

| Event | Fires on | Can affect outcome? | Notes |
|---|---|---|---|
| `create` | `bl create` | yes — same negotiation as the others | Describe-gated (SPEC §6.1): an old plugin that does not declare `create` is never invoked on it. Lets a tracker mirror new local tasks at birth instead of inferring them from a later `update`. |
| `claim` | `bl claim` | yes | Subscribed for legacy shim plugins iff `sync_on_change`. |
| `review` | `bl review` | yes | Subscribed for legacy shim plugins iff `sync_on_change`. |
| `update` | `bl update` (non-closing) | yes | Subscribed for legacy shim plugins iff `sync_on_change`. |
| `close` | `bl close`, `bl update status=closed` | yes | Subscribed for legacy shim plugins iff `sync_on_change`. |
| `sync` | `bl sync`, `bl prime` | yes | The standalone bidirectional event; runs for every enabled plugin regardless of per-event subscriptions. |
| `drop` | `bl drop` | **no — observe-only** (SPEC §6.2) | Best-effort notification; the propose response cannot block or alter the drop. Declaring `required` or `gating` on `drop` is a **config-validation error**: drop changes no durable Task, so there is nothing to roll back, gate, or stage. Only `best-effort` is legal. |

`create` and `drop` are both purely additive: subscribing is opt-in via `describe`, and a plugin that does not list them in `subscriptions` is never called on either event (no observe-death-without-birth asymmetry, but also no surprise invocations on older plugins).

#### `EventCtx` v1 (the describe-gated side channel)

Bare `propose` stdin carries only the post-image Task. That is insufficient for any real policy — the plugin can't see *who* moved the task, *what it was before*, or *which overrides were in play*. A native plugin that sets `wants_context: true` in its describe response receives an additional document at the path passed via `--ctx-file`. The legacy stdin shape is byte-unchanged for plugins that don't opt in (SPEC §5.1).

Schema (additive — unknown keys are ignored by a context-aware plugin, so a newer balls writing extra fields stays compatible with an older plugin):

```json
{
  "schema_version": 1,
  "event": "review",
  "actor": "alice",
  "repo": "git@github.com:example/repo.git",
  "overrides": ["--no-sync"],
  "task_before": { "...prior Task projection (the diff basis)..." },
  "commit": "abc123def..."
}
```

| Field | When set | Description |
|---|---|---|
| `schema_version` | always | Currently `1`. Bumped only on a breaking change; new keys are additive and do **not** bump it. |
| `event` | always | Lowercase wire name (`claim`, `review`, `close`, `update`, `create`, `drop`, `sync`). Matches the `--event` flag. |
| `actor` | always | The `BALLS_IDENTITY` / `--as` identity that invoked the command. |
| `overrides` | always (may be empty) | Per-invocation flags that applied to this event — e.g. `["--no-sync"]`, `["--skip=jira"]`, `["--required=jira"]`. The state-branch commit subject carries the same list (SPEC §11) for post-hoc audit. |
| `repo` | when known | Identity of the originating repo (for multi-repo hubs). |
| `task_before` | when known | The pre-image Task as JSON — the diff basis the post-image on stdin should be compared against. |
| `commit` | when known | The state-branch sha that recorded this event, once available. |

The file is removed by balls once the child exits; treat the path as ephemeral and read it eagerly.

#### Forward compatibility for plugin authors

The participant model crosses three serde seams across version boundaries. **The rule is one-line: unknown = round-trip, never die** (SPEC §13). Concretely for a plugin author:

- A newer balls may send `propose` an event name your plugin doesn't recognize. Don't crash — return `{}` (treated as `Other`) or skip the event silently if you don't handle it. Subscribing only to events you implement is the cleanest path.
- A newer balls may add keys to `EventCtx` (or to a subsequent `propose` stdin). Ignore unknown keys; don't fail to parse the document.
- An older balls may meet your `describe` response and not understand a new subscription or a new top-level key. Old balls drops the unknown from the subscription set or ignores the key, and the rest still works. You don't need to negotiate a version — just declare what you support.

The `task.extra` catch-all (SPEC §13 seam 1) preserves unknown fields across reads/writes too, so a Task projection that names a field this build doesn't know is round-tripped through, not silently dropped.

#### Where the formal contract lives

This subsection is plugin-author orientation; the authoritative contract — projection algebra, retry budget bounds, commit-policy composition rules, the `reject` veto's exact override semantics, conformance test list — is `docs/SPEC-lifecycle-sync-participants.md`. Read this for "how do I write a plugin"; read the SPEC for "exactly what does balls guarantee about my plugin."

### Sync lifecycle

When `sync_on_change` is true in config:

1. `bl create` → core creates task file, commits, then calls `plugin push --task ID` with the new task on stdin. Core reads the plugin's push response and writes it into `task.external.{plugin_name}`.
2. `bl close` → core closes task (archives the file), then calls `plugin push --task ID`. Push response is not written back since the task file is archived.
3. `bl update` → same pattern as create. Push response written back.
4. `bl sync` → after git sync, calls `plugin sync` with all local tasks on stdin. Core processes the sync report: creates new tasks, updates existing tasks, defers deleted tasks. Each operation is committed individually.

Core calls `auth-check` before every push or sync. If auth is expired (exit 1), core prints a warning and skips that plugin. Local operations are never blocked by plugin auth failures.

### Participant enforcement (`SPEC-lifecycle-sync-participants.md` §9/§11)

Each subscribed plugin negotiates the event under a per-event failure policy (`.balls/config.json` → `plugins.<name>.participant.subscriptions.<event>.policy`, one of `required` / `best-effort` / `gating`; legacy `sync_on_change` maps to `best-effort`). What the lifecycle command does with the outcome:

- **required** — a wire failure or a first-class `reject` (`SPEC-lifecycle-sync-participants.md` §8.1, a native `{"reject":{"reason":...}}`) **aborts the command**: the event's state-branch commit is rolled back so the task returns to its pre-event state, and `bl` exits non-zero with the plugin's reason verbatim. (`bl claim` rolls back by un-claiming; the review squash on `main` and the close worktree teardown are out of scope per the SPEC's staging.)
- **best-effort** — warn and continue; the event ships and the verbatim reason is recorded in `task.sync_status.<plugin>` (cleared on the next success). Legacy-shim skips stay silent so unmodified configs are byte-identical.
- **gating** — inert until the staging machinery lands (separate ball); the event ships, nothing is recorded.

Per-invocation overrides (`SPEC-lifecycle-sync-participants.md` §11), valid on `claim`/`review`/`close`/`update`/`create`:

| Flag | Effect |
|---|---|
| `--skip=NAME` | Drop participant `NAME` from this event — ships past a required veto. |
| `--required=NAME` | Force participant `NAME` to `required` for this event. |
| `--sync` / `--no-sync` | Force the git-remote participant on/off (as before). |

Every applied override is logged in the event's state-branch commit subject (e.g. `balls: update bl-a1b2 - title [--skip=jira]`) so a post-hoc audit shows which negotiations ran without their required participants. A `wants_context` native plugin additionally receives the pre-image (`task_before`), the event's `commit` sha, and the `overrides` list on its describe-gated EventCtx side channel (`SPEC-lifecycle-sync-participants.md` §5.1).

### Sync with `--task` filtering

`bl sync --task ID` passes the `--task` flag through to the plugin. The plugin filters its operations to just that item. The ID can be a local ball ID or a remote key — the plugin resolves which. Core processes the sync report the same way regardless of filtering.

### Conflict resolution between local and remote

- **Remote task created:** Plugin returns it in `sync.created`. Core creates local task file with `external.{plugin_name}` populated.
- **Local task created with `create_in_remote: true`:** Plugin creates remote issue during `push`, returns `remote_key` in push response. Core stores it in `task.external.{plugin_name}`.
- **Both sides edited:** The plugin decides conflict resolution in its `sync` implementation and returns the result in `updated`. Core applies field changes and notes.
- **Remote task deleted:** Plugin returns it in `sync.deleted`. Core marks local task as `deferred` with an explanatory note.
- **Local task closed:** Plugin receives the closed status via `push` and transitions the remote issue.

Core maintains a top-level `synced_at` map on every task, keyed by plugin name, pointing to the RFC3339 timestamp of the last successful push or sync-report application for that plugin. The map is sent back to the plugin on every subsequent push/sync — plugins compare their remote's `updated_at` against `task.synced_at.{plugin_name}` to decide whether the remote has changes they haven't yet applied locally, without maintaining their own side-cache under `auth-dir`. A missing key means "never synced". Failed pushes and failed sync-report entries leave the map untouched.

---

## Multi-Machine / Multi-Dev Operation

Each developer:

1. Clones the repo. `git clone` fetches `main` and the `balls/tasks` orphan branch automatically.
2. Runs `bl init` once per clone. This checks out the state branch into `.balls/worktree/`, creates the `.balls/tasks` symlink, and seeds `.balls/local/` for ephemeral state.
3. Runs `bl sync` to stay current — pulls both main and the state branch from origin.
4. Claims tasks, works in bl worktrees, runs `bl review` to deliver.

A developer and their agents on one machine are just workers sharing the `.balls/local/` cache and a single state worktree. Remote developers are workers on different machines sharing state through git. The coordination model is the same: optimistic concurrency, conflict at merge time, resolution via the text-mergeable schema and the field-wise resolver.

### Parallel workers on one machine

Multiple agent processes running simultaneously on the same clone are safe. The per-task flock at `.balls/local/lock/<id>.lock` serializes writes on a single task, and the store-wide flock at `.balls/local/lock/state-worktree.lock` serializes writes to the state branch's git index so concurrent `bl create` / `bl claim` / `bl review` calls don't race on `.balls/worktree/.git/index.lock`. Empirically: without the store-wide lock, 6 of 8 parallel `bl create` workers fail with `fatal: Unable to create index.lock`; with the lock, 8 of 8 succeed.

There is no central server. There is no daemon. Git is the coordination layer. Plugins talk to external services when configured, but the core system operates without them.

---

## User Stories

### Setup

1. Initialize balls in an existing git repo. Creates directory structure, gitignore entries, initial commit.
2. Initialize in a repo that already has balls initialized. No-op, prints "already initialized."
3. Clone a repo that has balls tasks. `.balls/tasks/` is present; `bl init` creates only local ephemeral dirs.

### Task Creation

4. Create a task with title only. Generates ID, writes file with defaults (type=task, priority=3, status=open), commits.
5. Create a task with all options (priority, type, parent, deps, tags, description). All fields populated correctly.
6. Create a task with a dependency on a nonexistent ID. Rejected with error.
7. Create a task as child of a parent. `parent` field set. Parent file is NOT modified (children are computed on read).
8. Create a task with a circular dependency. Rejected with error.
9. Create a task when plugin sync is enabled. Task file committed, then plugin push called with task data. Plugin failure does not roll back the local create.

### Listing and Querying

10. List all open tasks. Shows non-closed tasks sorted by priority, then `created_at`.
11. List tasks filtered by status.
12. List tasks filtered by priority.
13. List tasks filtered by tag.
14. List children of a parent task.
15. Show a single task with full detail, including computed blocked status and children list.
16. List all tasks including closed (`--all`).

### Ready Queue

17. Compute ready queue with no dependencies. All open unclaimed tasks returned, sorted by priority.
18. Compute ready queue with dependencies. Only tasks whose deps are all closed appear.
19. Ready queue excludes claimed tasks.
20. Ready queue auto-fetches when local state exceeds stale threshold.
21. Ready queue with `--no-fetch` skips fetch even if stale.

### Claiming and Worktrees

22. Claim a ready task. Task file updated (claimed_by, status=in_progress, branch), committed. Worktree created. Local cache symlinked. Worktree path printed.
23. Claim a task already claimed locally. Rejected with error.
24. Claim a task with unmet dependencies. Rejected with error.
25. Claim a closed task. Rejected with error.
26. Claim a task when worktree directory already exists (stale). Rejected, suggests `bl drop`.
27. Worktree has access to `.balls/local/` via symlink.
28. Claim with explicit worker identity (`--as`).
29. Claim triggers plugin push if configured.

### Working in a Worktree

30. Code changes in worktree are on the task's branch, isolated from main and other worktrees.
31. `bl show` works from within a worktree.
32. `bl update` with `--note` from within a worktree appends note and commits.

### Closing Tasks

33. Close a task. Task archived on the state branch (file removed from tip, preserved in history), bl worktree removed, local claim cleaned. Main is not touched by close.
34. Close with a message. Message appears in notes.
35. Close a task that is a dependency of another. Dependent task now appears in `bl ready`.
36. Close the last child of a parent. Parent's computed completion reaches 100%.
37. Close triggers plugin push if configured.

### Dropping Tasks

38. Drop a claimed task. Task reset to open, worktree removed, local claim removed, committed.
39. Drop with uncommitted work. Warns. Requires `--force`. Work is lost.

### Dependencies

40. Add a dependency. `depends_on` updated, committed.
41. Add a dependency creating a cycle. Rejected.
42. Remove a dependency. Committed.
43. View dependency tree for one task.
44. View full project dependency graph.

### Syncing

45. Sync with no remote changes. Fetch, no merge needed, push local commits.
46. Sync with non-conflicting remote changes. Clean merge and push.
47. Sync with conflicting task files. Auto-resolve per rules (status precedence, union notes, later timestamp wins). Commit resolution and push.
48. Sync when offline. Fetch fails gracefully. All local operations continue. Push deferred.
49. Sync triggers plugin sync if configured.

### Conflict Resolution

50. Two workers claim same task on different machines. First push wins. Second worker's `bl sync` detects conflict, resets their claim, suggests next ready task.
51. Two workers close same task. First merge wins. Second prompted to file new task with their changes or discard.
52. One worker closes, another updates. Closed status wins. Update's notes preserved.
53. Different tasks edited concurrently. No conflict — different files.

### Multi-Dev Workflow

54. Dev A creates tasks and runs `bl sync`, pushing both main and `balls/tasks`. Dev B clones (git fetches both branches), runs `bl init` to set up the state worktree + symlink, runs `bl list` and sees all tasks.
55. Dev A claims task, pushes. Dev B's `bl ready` does not show that task.
56. Multiple devs running agent swarms. Each agent claims distinct tasks. Git push serializes merges.
57. New dev joins, clones, runs `bl init`. Full task state available immediately.
58. Dev works offline for a day. Creates and closes tasks. Comes online, `bl sync` resolves conflicts.

### Agent Lifecycle

59. Agent starts, runs `bl prime`. Gets synced state, ready queue, any in-progress tasks for this identity.
60. Agent picks top ready task, claims it, works in worktree.
61. Agent finishes, runs `bl review`. Work squash-merged to main as one `[bl-xxxx]`-tagged feature commit, worktree stays, status=review on the state branch, delivered_in hint set.
62. Reviewer approves, runs `bl close` from repo root. Task archived, worktree removed.
63. Reviewer rejects, runs `bl update ID status=in_progress --note "reason"`. Agent resumes in existing worktree, next `bl review` merges main first.
64. Agent session ends mid-task (context overflow). New session, `bl prime` shows task still claimed by this identity. Agent resumes in existing worktree.
65. Agent crashes. Task stays in_progress. Human or supervisor runs `bl drop` to release.

### Plugin System

64. Configure Jira plugin. Write `.balls/plugins/jira.json`, run `balls-plugin-jira auth-setup`.
65. Create task with plugin sync enabled. Task created locally, then pushed to Jira. `external.jira.key` populated.
66. Close task with plugin sync enabled. Jira issue transitioned to Done.
67. Run `bl sync` with plugin. Bidirectional: new Jira issues become local tasks, local changes pushed to Jira.
68. Jira issue created by someone else. After `bl sync`, local task file exists with `external.jira.key` set.
69. Jira issue deleted. After `bl sync`, local task marked deferred with explanatory note.
70. Plugin auth expires. `auth-check` returns 1. `bl sync` warns "Jira plugin: auth expired, run `balls-plugin-jira auth-setup`." Local operations unaffected.
71. Plugin is unavailable (network down). Sync skips plugin, warns, continues with git-only sync.
72. Plugin config committed to repo. New dev clones, gets config. Runs `auth-setup` once to provide their own credentials.

### Edge Cases

73. Create task in a repo with no commits. `bl init` creates initial commit first.
74. Run `bl` outside a git repo. Error: "not a git repository."
75. Run `bl` in repo without `.balls/`. Error: "not initialized. Run `bl init`."
76. Malformed task JSON. Error on read, suggests `bl repair`.
77. Worktree creation fails (disk full, permissions). Claim rolled back (task file reverted, local claim removed).
78. Hundreds of tasks. Performance is fine — ls + jq on hundreds of small JSON files is milliseconds.
79. Task ID collision. Auto-retry with incremented timestamp.

---

## Radical Simplicity

Balls's thesis: every layer of infrastructure you add is a layer that can break, a layer to learn, a layer to operate. The best tool is the one with the fewest moving parts that solves the problem.

**The CLI is the agent interface.** Agents already have shell access. `bl ready --json` is a tool call. There is no need for MCP servers, REST APIs, or protocol adapters. If you can run a command, you can use balls.

**Git is the archive.** Closed tasks are removed from the state branch's tip and preserved in its history. There is no compaction, no garbage collection, no cleanup threshold. Only live tasks exist in the working set. Old tasks are retrievable via `git log balls/tasks` when needed.

**Git is the database.** Task files are committed, pushed, pulled, and merged like code — on a dedicated orphan branch inside your existing repo. There is no second version-control system to reconcile, no schema migrations, no embedded database engine. If you understand git, you understand balls's storage model.

---

## Why Not Existing Alternatives

### Beads

Beads was right about the core insight: agents need structured, queryable, persistent task state — not markdown files strewn across a repo. Balls is built on the same realization, and the two projects agree on more than they disagree on. Both keep task state out of the main branch's commit history so feature work and bookkeeping don't interleave; balls does this with an orphan git ref, beads does it with a separate database. The question we answer differently is what holds that state.

Beads uses Dolt — a version-controlled SQL database — as the backing store. That buys cell-level merging and sub-millisecond queries, both genuinely nice properties on large task sets. The cost is running two version-control systems side by side: git for code, Dolt for tasks. That's two histories to keep consistent, two merge models to learn, two remotes to push to, and a separate database binary every collaborator has to install. The jsonl export mode exists but isn't the shared source of truth, so sharing state without Dolt is a second-class path.

Balls asks whether one VCS can do both jobs. The orphan-ref design keeps task data fully out of main's commit graph — same separation beads gets from Dolt — but stores it in the same git repository, fetched by the same `git fetch`, pushed by the same `git push`. A collaborator who clones the repo gets the backlog automatically; a collaborator without `bl` installed can still read, diff, and hand-edit task files with stock git. There is no second system to operate.

This is a tradeoff, not a free win. Dolt's cell-level merge is strictly more granular than git's file-level merge, and at the scale where per-field conflict resolution really matters, Dolt has the stronger answer. Balls mitigates the file-level constraint with a text-mergeable JSON schema and an append-only notes sidecar — disjoint-field edits and concurrent note appends merge cleanly under stock git — but it doesn't match Dolt's per-cell precision.

The bet is that one VCS beats two whenever one is sufficient, and that for tracking a backlog of tasks, git is sufficient.

### Cline Kanban

Cline Kanban provides a visual board for agent orchestration with worktree-per-task isolation. It solves the human attention problem well. But it's local-only with no multi-machine story, closed-source infrastructure, and tightly coupled to the Cline ecosystem despite claiming agent-agnosticism. There is no durable shared state — each developer's board is independent.

### GitHub Issues / Jira / Linear

Traditional trackers weren't designed for agent workflows. They require network round-trips for every read, can't be queried offline, don't support the claim-and-worktree lifecycle, and have no concept of local-first operation. They remain the right tools for human project management. Ball integrates with them via *issue-tracker* plugins rather than replacing them.

Don't confuse that with a **forge plugin**, which is a different integration entirely: a forge plugin gates *delivery* (opening the PR on `bl review`, closing the gate child when the forge merges) in deferred mode — see *Delivery Modes*. Same plugin protocol, unrelated job; issue-tracker plugins mirror backlog state, forge plugins drive the merge gate. Forge plugins ship separately, per-forge, and are not bundled in this repo.

### The balls approach

Ball takes the core insight — structured task files, dependency tracking, agent-native CLI — and implements it on the only infrastructure every developer already has: git. Tasks are files. Sync is push/pull. History is git log. Collaboration is merge. There is nothing to install except a small CLI, nothing to configure except a JSON file, and nothing to operate except git.
