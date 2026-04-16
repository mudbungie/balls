# balls

**balls** is a git-native task tracker for parallel agent workflows. Tasks are JSON files committed to your repo. Worktrees provide isolation. Git provides sync, history, and collaboration. There is no database, no daemon, no external service.

The CLI is `bl`. Every `bl` operation is expressible as file edits and git commands. The system is designed for a single developer running many agents, multiple developers each running many agents, and anything in between. It works offline. It degrades gracefully.

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

`make install` builds a release binary, installs `bl` to `~/.local/bin/`, and installs shell completions to `~/.local/share/`. Make sure `~/.local/bin` is on your `PATH`.

`make hooks` wires up the repo-local pre-commit hook (clippy, line-length cap, tests, 100% coverage). Run it once per clone; it's not part of `make install` because a user installing the binary shouldn't have hooks attached to whatever repo they happen to be in. The coverage check requires `cargo install cargo-tarpaulin`.

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
2. On every push to `main`, `.github/workflows/release-plz.yml` opens (or updates) a **Release PR** that bumps `Cargo.toml`, regenerates `CHANGELOG.md`, and lists the commits going into the release. Because commits are not Conventional Commits, release-plz defaults to patch bumps — hand-edit the version in the Release PR if you want a minor or major bump.
3. Review the Release PR. CI (`.github/workflows/ci.yml`) runs `cargo test`, `cargo clippy`, line-length + 100% coverage checks, and `cargo publish --dry-run` against it.
4. Merge the Release PR. release-plz tags `vX.Y.Z`, creates a GitHub release, and publishes to crates.io.

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
8. **The CLI is a convenience, not a requirement.** Every operation is expressible as file edits + standard git commands. A human with `vim`, `ln`, and `git` can do everything `bl` does — SPEC §11 publishes the shell sequences.
9. **Plugins extend, core stays small.** External integrations (Jira, Linear, GitHub Issues) are handled by a plugin interface. Auth, sync logic, and API specifics never enter the core.

---

## Glossary

| Term | Meaning |
|---|---|
| **task** | A unit of work. One JSON file on the `balls/tasks` orphan branch, exposed to main via a symlink. |
| **state branch** | The orphan git branch `balls/tasks` that holds all task state. No shared history with main. |
| **state worktree** | A second git worktree at `.balls/worktree/` with the state branch checked out. Where task files physically live. |
| **ready** | A task that is open, has all dependencies met, and is unclaimed. |
| **claim** | Taking ownership of a task. Creates a git worktree under `.balls-worktrees/<id>/` for the work. |
| **review** | Submitting work: squash-merges the worker's branch into main as a single feature commit tagged `[bl-xxxx]`, and flips the task to `review` on the state branch. |
| **close** | Approving completed work. Archives the task on the state branch and removes the bl worktree. |
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

### .gitignore entries

`bl init` adds these to main's `.gitignore`:

```
.balls/local
.balls/tasks
.balls/worktree
.balls-worktrees
```

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
  "delivered_in": null
}
```

Notes live in a sibling file `<id>.notes.jsonl` rather than in the task.json. That split is an architectural invariant — see Text-Mergeable Schema below.

### Field definitions

| Field | Type | Description |
|---|---|---|
| `id` | string | Format `bl-XXXX` (4 hex chars by default). Generated from sha1 of title + timestamp, truncated. |
| `title` | string | Human-readable summary. |
| `type` | enum | `epic`, `task`, `bug`. |
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
| `delivered_in` | string? | SHA of the main-branch squash commit that delivered this task. Written by `bl review`. Performance hint only — ground truth is the `[bl-xxxx]` tag in the commit subject. See Delivery Link. |

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

### bl init [--stealth]

One-time setup per clone. `bl init` is idempotent and self-healing — running it on an already-initialized repo verifies and repairs. Specifically:

1. Creates `.balls/local/`, `.balls/plugins/`, `.balls/config.json` and adds the gitignore entries.
2. Creates or fetches the `balls/tasks` orphan branch. If the branch exists on `origin`, it's tracked; otherwise a fresh orphan is created and pushed (best-effort) so subsequent clones discover it.
3. Checks the state branch out as a second git worktree at `.balls/worktree/`.
4. Seeds `.balls/tasks/.gitattributes` with `*.notes.jsonl merge=union` on the state branch.
5. Creates the `.balls/tasks → worktree/.balls/tasks` symlink in main's working tree.
6. Commits the main-side additions (`.gitignore`, `config.json`, `plugins/.gitkeep`) as a single `balls: initialize` commit.

With `--stealth`, tasks are stored outside the repo at `~/.local/share/balls/<repo-hash>/tasks/` with no state branch at all. Useful for local-only planning that shouldn't appear in any git history. All other bl commands work identically; the orphan-branch topology is simply bypassed.

**By hand:** see SPEC §11 for the full shell sequence (`git switch --orphan balls/tasks`, `git worktree add .balls/worktree balls/tasks`, `ln -s worktree/.balls/tasks .balls/tasks`, gitignore updates, initial commit).

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
bl list --all              # including closed
```

Reads task files, filters, sorts by priority then `created_at`.

**By hand:** `for f in .balls/tasks/bl-*.json; do jq '.' "$f"; done | jq -s 'sort_by(.priority, .created_at)'`

### bl ready

```
bl ready                   # list ready tasks
bl ready --json            # machine-readable
```

Computes the ready queue. Auto-fetches if local state is older than `stale_threshold_seconds` from config (default 60s). `--no-fetch` to skip.

**By hand:** List open tasks, filter to those with all deps closed and no `claimed_by`, sort by priority.

### bl show ID

```
bl show bl-a1b2
bl show bl-a1b2 --json
```

Displays one task with computed fields — blocked status, children if parent, dependency chain — and the resolved delivery link if the task has been delivered. The delivery line looks like `delivered: e69193f Add bl completions... [bl-1a34]`; if the cached `delivered_in` SHA is stale, the tag scan on main still finds the commit and the display is annotated `(hint stale)`. `--json` exposes `delivered_in_resolved` and `delivered_in_hint_stale` alongside the task.

**By hand:** `cat .balls/tasks/bl-a1b2.json | jq .` — the symlink transparently reads from the state worktree.

### bl claim ID [--as IDENTITY]

```
bl claim bl-a1b2
bl claim bl-a1b2 --as dev1/agent-alpha
```

Validates the task is claimable → flips status/claimed_by/branch on the state branch → commits (`balls: claim bl-a1b2`) → creates a git worktree at `.balls-worktrees/bl-a1b2/` on `work/bl-a1b2` → symlinks `.balls/local`, `.balls/worktree`, and `.balls/tasks` into the new worktree → writes the local claim file → prints the worktree path. Triggers plugin push if configured.

Fails if already claimed locally, deps unmet, or task not `open`.

### bl review ID [-m MSG]

```
bl review bl-a1b2 -m "$(cat <<'EOF'
Short title under ~50 chars

Body paragraph explaining the change in detail. Wrap at ~72.
Multiple paragraphs are preserved as the commit body.
EOF
)"
```

Worker's exit point. Commits uncommitted work in the bl worktree → merges main in (surfaces conflicts there, not on main) → squash-merges to main as a single feature commit → writes the `delivered_in` hint and flips the task to `review` on the state branch in one commit. The worktree and the claim stay intact so a rejected review can be reworked in place.

Commit messages use 50/72 shape: the first line of `-m` becomes the commit title with `[bl-xxxx]` appended, and everything after the first newline becomes the body. A single-line `-m "fix foo"` still works (no body). Don't stuff a multi-sentence summary into a single line — `git log --oneline` becomes unreadable.

If the reviewer rejects (`bl update bl-a1b2 status=in_progress`), the worker resumes in the existing bl worktree and calls `bl review` again; the next run merges main first, picking up the rejection.

### bl close ID [-m MSG]

```
bl close bl-a1b2 -m "approved"
```

Reviewer approval. Removes the bl worktree, deletes `work/bl-a1b2`, and archives the task on the state branch (parent bookkeeping, `git rm` of `.json` and `.notes.jsonl`, and the `state: close` commit in one atomic locked sequence). **Rejects if run from inside the worktree** — must run from the repo root, which `bl close` prints on success so you can `cd` back.

The reviewer message is embedded in the state-branch close commit's body (not appended to a notes file, which is about to be deleted). It's still in git history on `balls/tasks`.

### bl update ID [field=value ...] [--note TEXT]

```
bl update bl-a1b2 priority=2
bl update bl-a1b2 status=blocked --note "Waiting on API team"
bl update bl-a1b2 status=closed        # closing unclaimed tasks skips the bl close path
```

Edits fields directly on the state branch (no bl worktree required) and commits `balls: update bl-a1b2 - title`. Notes are appended to the sibling `.notes.jsonl` file. `status=closed` on an unclaimed task goes through the same atomic archive as `bl close`.

**By hand:** see SPEC §11 for the canonical edit-and-publish shell sequence (`$EDITOR .balls/tasks/bl-a1b2.json; git -C .balls/worktree add .balls/tasks/bl-a1b2.json; git -C .balls/worktree commit -m "bl-a1b2: bumped priority"`).

### bl drop ID [--force]

Releases a claim. Resets task file to open/unclaimed/no-branch, removes worktree, removes local claim, commits. `--force` required if worktree has uncommitted changes (they are lost).

**By hand:** Edit task JSON, `git worktree remove`, `rm` claim file, commit.

### bl dep add TASK DEPENDS_ON

Appends to `depends_on`. Rejects cycles. Commits.

### bl dep rm TASK DEPENDS_ON

Removes from `depends_on`. Commits.

### bl dep tree [ID]

Walks `depends_on` and `parent` relationships. Prints indented tree with status indicators. Without ID, shows full project graph.

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

**By hand:** see SPEC §11. The shell sequence is two `git -C .balls/worktree push origin balls/tasks` plus a `git push origin main`, with `git fetch` and `git merge` between as needed.

### bl prime [--as IDENTITY]

Session bootstrap for agents. Runs `bl sync`, then outputs:
- Worker identity
- Ready tasks ranked by priority
- Currently claimed tasks for this identity (for session resume)

Designed to be injected into an agent's context at session start.

### bl resolve FILE

Manual conflict resolution helper: parses both sides of a conflicted task file, applies the field-wise resolution rules, writes the result. Rarely needed in the new topology — most conflicts merge cleanly under stock git — but available for edge cases.

---

## Config (.balls/config.json)

Committed to main, shared across the team.

```json
{
  "version": 1,
  "id_length": 4,
  "stale_threshold_seconds": 60,
  "auto_fetch_on_ready": true,
  "worktree_dir": ".balls-worktrees",
  "tasks_dir": null,
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
| `id_length` | Hex chars in generated task IDs. Clamped to `[4, 32]` on load; out-of-range values produce a warning and fall back to 4. |
| `stale_threshold_seconds` | `bl ready` auto-fetches if the last fetch is older than this. |
| `auto_fetch_on_ready` | Whether `bl ready` auto-fetches at all. |
| `worktree_dir` | Where `bl claim` creates worktrees. Must be a relative path under the repo; values containing `..` or starting with `/` are rejected on load. |
| `tasks_dir` | Stealth-mode override for the task storage location. Normally null; set by `bl init --stealth` to an external path outside the repo. |
| `plugins` | Per-plugin enable/sync flags and config file paths. |

### Environment overrides

| Variable | Purpose | Default |
|---|---|---|
| `BALLS_IDENTITY` | Worker identity for claims and notes | `$USER`, then `"unknown"` |
| `BALLS_PLUGIN_TIMEOUT_SECS` | Wall-clock cap on any plugin invocation | 30 |
| `BALLS_PLUGIN_MAX_STREAM_BYTES` | Max bytes buffered from a plugin's stdout/stderr | 1 MiB |

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
| `type` | no | `"task"` | One of: `epic`, `task`, `bug` |
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

### Sync lifecycle

When `sync_on_change` is true in config:

1. `bl create` → core creates task file, commits, then calls `plugin push --task ID` with the new task on stdin. Core reads the plugin's push response and writes it into `task.external.{plugin_name}`.
2. `bl close` → core closes task (archives the file), then calls `plugin push --task ID`. Push response is not written back since the task file is archived.
3. `bl update` → same pattern as create. Push response written back.
4. `bl sync` → after git sync, calls `plugin sync` with all local tasks on stdin. Core processes the sync report: creates new tasks, updates existing tasks, defers deleted tasks. Each operation is committed individually.

Core calls `auth-check` before every push or sync. If auth is expired (exit 1), core prints a warning and skips that plugin. Local operations are never blocked by plugin auth failures.

### Sync with `--task` filtering

`bl sync --task ID` passes the `--task` flag through to the plugin. The plugin filters its operations to just that item. The ID can be a local ball ID or a remote key — the plugin resolves which. Core processes the sync report the same way regardless of filtering.

### Conflict resolution between local and remote

- **Remote task created:** Plugin returns it in `sync.created`. Core creates local task file with `external.{plugin_name}` populated.
- **Local task created with `create_in_remote: true`:** Plugin creates remote issue during `push`, returns `remote_key` in push response. Core stores it in `task.external.{plugin_name}`.
- **Both sides edited:** The plugin decides conflict resolution in its `sync` implementation and returns the result in `updated`. Core applies field changes and notes.
- **Remote task deleted:** Plugin returns it in `sync.deleted`. Core marks local task as `deferred` with an explanatory note.
- **Local task closed:** Plugin receives the closed status via `push` and transitions the remote issue.

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

Traditional trackers weren't designed for agent workflows. They require network round-trips for every read, can't be queried offline, don't support the claim-and-worktree lifecycle, and have no concept of local-first operation. They remain the right tools for human project management. Ball integrates with them via plugins rather than replacing them.

### The balls approach

Ball takes the core insight — structured task files, dependency tracking, agent-native CLI — and implements it on the only infrastructure every developer already has: git. Tasks are files. Sync is push/pull. History is git log. Collaboration is merge. There is nothing to install except a small CLI, nothing to configure except a JSON file, and nothing to operate except git.
