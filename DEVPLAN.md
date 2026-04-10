# ball — Development Plan

## Crate Setup

```
cargo init ball
```

### Dependencies (Cargo.toml)

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
sha1 = "0.10"
hex = "0.4"
fs2 = "0.4"                # flock

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
```

No async. No tokio. No reqwest. Plugins handle their own networking.

### Project Structure

```
src/
  main.rs
  cli.rs           # clap derive structs
  task.rs          # Task struct, ID gen, serialization
  store.rs         # Read/write/list task files, flock
  git.rs           # Subprocess calls to git
  ready.rs         # Ready queue, dep tree, cycle detection
  worktree.rs      # Create/destroy worktrees, symlink local
  resolve.rs       # Conflict resolution logic
  plugin.rs        # Plugin exec interface, sync lifecycle
  config.rs        # Config struct, read/write
  error.rs         # Error types
tests/
  integration/
    init.rs
    create.rs
    claim_close.rs
    ready.rs
    sync.rs
    conflict.rs
    plugin.rs
```

---

## Phases

### Phase 1: Foundation

Goal: `bl init`, `bl create`, `bl list`, `bl show`. A human can create and view tasks.

1. **error.rs** — Define `BallError` enum (IoError, GitError, TaskNotFound, InvalidTask, AlreadyInitialized, NotInitialized, NotARepo). Implement `Display`, `From<io::Error>`, `From<serde_json::Error>`.

2. **config.rs** — `Config` struct with serde. `Config::default()`. `Config::load(path)`, `Config::save(path)`. Keep it minimal: version, id_length, stale_threshold_seconds, worktree_dir, plugins map.

3. **git.rs** — Wrapper functions, all calling `std::process::Command`:
   - `git_root() -> Result<PathBuf>` (rev-parse --show-toplevel)
   - `git_add(paths: &[&Path])`
   - `git_commit(message: &str)`
   - `git_fetch(remote: &str) -> Result<()>` (allow failure)
   - `git_merge(branch: &str) -> Result<MergeResult>` (clean, conflict, up-to-date)
   - `git_push(remote: &str) -> Result<()>`
   - `git_worktree_add(path: &Path, branch: &str)`
   - `git_worktree_remove(path: &Path)`
   - `git_branch_delete(branch: &str)`
   - `git_has_remote() -> bool`
   - `git_init_commit()` (for repos with no commits)

   Every function returns `Result<T, BallError>`. Capture stderr for error messages.

4. **task.rs** — `Task` struct with all fields from spec. `Task::new(title, opts)` generates ID via sha1(title + timestamp), truncated to `id_length` hex chars. Retry on collision. `Task::load(path)`, `Task::save(path)` (write to tmp, `mv` into place for atomicity).

5. **store.rs** — `Store` struct holds the repo root path.
   - `Store::discover() -> Result<Store>` (walk up to find `.ball/`)
   - `Store::init(path) -> Result<Store>` (create dirs, config, gitignore, commit)
   - `store.tasks_dir() -> PathBuf`
   - `store.local_dir() -> PathBuf`
   - `store.load_task(id) -> Result<Task>`
   - `store.save_task(task) -> Result<()>` (with flock via fs2)
   - `store.all_tasks() -> Result<Vec<Task>>`
   - `store.task_exists(id) -> bool`

6. **cli.rs** — Clap derive for top-level commands. Start with Init, Create, List, Show.

7. **main.rs** — Match on cli command, dispatch.

**Tests:** User stories 1–16 plus 73–75. Use `tempfile::TempDir`, init a git repo, run `bl` commands via `assert_cmd`.

### Phase 2: Dependencies and Ready Queue

Goal: `bl ready`, `bl dep add`, `bl dep rm`, `bl dep tree`. Agents can discover work.

8. **ready.rs** —
   - `ready_queue(tasks: &[Task]) -> Vec<&Task>` — filter open, unclaimed, all deps closed. Sort by priority, created_at.
   - `dep_tree(tasks: &[Task], root_id: &str) -> Tree` — walk depends_on and parent. Simple recursive struct.
   - `detect_cycle(tasks: &[Task], from: &str, to: &str) -> bool` — DFS on depends_on graph.
   - `children_of(tasks: &[Task], parent_id: &str) -> Vec<&Task>`
   - `completion(tasks: &[Task], parent_id: &str) -> f64`

9. Wire `bl dep add`, `bl dep rm`, `bl dep tree` into CLI. `dep add` calls `detect_cycle` before writing. `dep rm` just removes the entry.

10. Wire `bl ready`. Add `--json` flag. Add auto-fetch logic: check mtime of `.ball/local/last_fetch` against `stale_threshold_seconds`, fetch if stale.

**Tests:** User stories 17–21, 40–44. Test cycle detection with diamond deps, self-deps, transitive cycles.

### Phase 3: Claim, Close, Drop — Worktree Lifecycle

Goal: `bl claim`, `bl close`, `bl drop`. The core workflow loop.

11. **worktree.rs** —
    - `create_worktree(store, task_id, identity) -> Result<PathBuf>`:
      1. Acquire flock on `.ball/local/lock/{id}.lock` (fs2 `File::lock_exclusive`)
      2. Check `.ball/local/claims/{id}` doesn't exist
      3. Load task, validate claimable (open, deps met, unclaimed)
      4. Update task (claimed_by, status=in_progress, branch=work/{id})
      5. Save task, git add, git commit
      6. `git worktree add`
      7. Symlink `.ball/local` into worktree's `.ball/local`
      8. Write claim file
      9. Release lock (drop File)
      10. Return worktree path

    - `close_worktree(store, task_id, message) -> Result<()>`:
      1. Load task from worktree
      2. Update task (status=closed, closed_at, append note if message)
      3. In worktree: git add -A, git commit
      4. Back in main: git merge work/{id}
      5. git worktree remove
      6. rm claim file
      7. git branch -d

    - `drop_worktree(store, task_id, force) -> Result<()>`:
      1. Check for uncommitted changes (git status in worktree). Reject unless force.
      2. Reset task (status=open, claimed_by=null, branch=null)
      3. Save, add, commit
      4. git worktree remove
      5. rm claim file
      6. git branch -D (force delete)

12. Wire into CLI: `bl claim`, `bl close`, `bl drop`.

**Tests:** User stories 22–39, 60–64. Test the full cycle: create → claim → modify files in worktree → close → verify merged. Test crash recovery (story 63–64). Test drop with dirty worktree.

### Phase 4: Sync and Conflict Resolution

Goal: `bl sync`, `bl resolve`. Multi-dev works.

13. **resolve.rs** —
    - `resolve_conflict(ours: &Task, theirs: &Task) -> Task`:
      1. Status: max by precedence (closed > in_progress > blocked > open > deferred)
      2. Notes: union by timestamp
      3. Timestamps: later updated_at wins for non-status fields
      4. claimed_by: from closing side if closed, else first writer
    - `parse_conflict_markers(file_content: &str) -> Result<(String, String)>` — extract ours/theirs from git conflict markers

14. **bl sync** implementation:
    1. `git fetch origin` (tolerate failure)
    2. `git merge origin/main` (or whatever the default branch is)
    3. If conflicts: for each conflicted file in `.ball/tasks/`, parse both sides, resolve, write, `git add`
    4. If resolved: `git commit`
    5. `git push origin` — if fails, fetch+merge+retry once
    6. Call plugin sync if configured

15. **bl resolve** — manual single-file resolution. Reads conflict markers, applies rules, writes clean file.

**Tests:** User stories 45–58. These are the hardest tests. You need to simulate two independent repos (or two worktrees of the same repo) making conflicting changes, then running sync. Use `git clone --bare` + two working clones to simulate remotes.

### Phase 5: Plugin System

Goal: `bl sync` triggers external plugins. Jira plugin specced but implemented separately.

16. **plugin.rs** —
    - `Plugin` struct: name, executable path (resolved via `which ball-plugin-{name}`), config path, auth dir.
    - `plugin.auth_check() -> Result<bool>` — run `ball-plugin-{name} auth-check`, check exit code.
    - `plugin.push(task: &Task) -> Result<()>` — serialize task to stdin, run `ball-plugin-{name} push --task {id} --config {path} --auth-dir {dir}`.
    - `plugin.pull() -> Result<Vec<ExternalTask>>` — run pull, parse JSON stdout.
    - `plugin.sync() -> Result<SyncReport>` — run sync, parse JSON stdout.
    - `run_plugin_push(store, task_id)` — called after create/close/update if sync_on_change.
    - `run_plugin_sync(store)` — called at end of `bl sync`.
    - All plugin calls: tolerate failure. Log warning, don't block core operations.

17. Wire plugin hooks into create, close, update, sync commands.

18. Update `bl init` to create `.ball/local/plugins/` dir.

**Tests:** User stories 64–72. Mock plugin as a shell script that records calls and returns canned responses.

### Phase 6: Agent Ergonomics

Goal: `bl prime`, `bl update`, final polish.

19. **bl prime** — Run sync, then output structured block:
    ```
    === ball prime: agent-alpha ===
    Claimed (resume): bl-a1b2 "Implement auth" @ .ball-worktrees/bl-a1b2
    Ready:
      [P1] bl-c3d4 "Fix database migration"
      [P2] bl-e5f6 "Add rate limiting"
    ===
    ```
    With `--json` for machine consumption.

20. **bl update** — Parse `field=value` pairs from args. Validate field names and value types. Special handling for `--note` (append to notes array). Commit. Plugin push.

21. **Protected main mode** — `config.protected_main: bool`. When true, all writes go through: create temp worktree → edit → commit → merge back → remove temp worktree. Wrap in a helper: `store.write_with_worktree(f: impl FnOnce(&Path))`.

22. **bl repair** — Scan `.ball/tasks/`, validate each JSON file, report/fix malformed ones. Clean up orphaned claim files where no worktree exists. Clean up orphaned worktrees where no claim exists.

**Tests:** User stories 59–63, 76–79.

---

## Test Strategy

### Unit tests (in-module)

- `task.rs`: ID generation, collision retry, serialization roundtrip
- `ready.rs`: Ready queue filtering, cycle detection, dep tree construction
- `resolve.rs`: All conflict resolution rules, edge cases

### Integration tests (tests/integration/)

Each test creates a temp dir, inits a git repo, and runs `bl` as a subprocess via `assert_cmd`. Validates file contents, git log, and stdout.

Key integration scenarios:
- **Full lifecycle:** init → create 3 tasks with deps → claim first → close → ready shows second → claim → close → ready shows third
- **Concurrent claims:** Two processes try to claim same task. One succeeds, one fails. (Use flock contention.)
- **Remote conflict:** Two cloned repos, both modify same task, push/sync/resolve.
- **Plugin round-trip:** Mock plugin, create task, verify push called, run sync, verify pull processed.
- **Protected main:** Enable flag, create task, verify main branch has no direct commits from bl (all come via merge).

### CI

GitHub Actions, Linux only initially. `cargo test`. No external services needed — everything is local git repos in temp dirs.

---

## Build and Distribution

```bash
cargo build --release        # ~3MB binary
```

### Install methods

1. **curl pipe:** `curl -fsSL https://github.com/you/ball/releases/latest/download/bl-$(uname -s)-$(uname -m) -o /usr/local/bin/bl && chmod +x /usr/local/bin/bl`
2. **cargo install:** `cargo install ball`
3. **Homebrew:** tap with formula pointing to GitHub releases

### Cross-compilation

```bash
cargo install cargo-zigbuild
cargo zigbuild --release --target x86_64-unknown-linux-gnu
cargo zigbuild --release --target aarch64-unknown-linux-gnu
cargo zigbuild --release --target x86_64-apple-darwin
cargo zigbuild --release --target aarch64-apple-darwin
```

GitHub Actions release workflow: on tag push, build all four targets, attach to release.

---

## Milestones

| Phase | Deliverable | Stories Covered | Estimated Effort |
|-------|-------------|-----------------|------------------|
| 1 | Create and view tasks | 1–16, 73–79 | Day 1 |
| 2 | Deps and ready queue | 17–21, 40–44 | Day 1 |
| 3 | Claim/close/drop lifecycle | 22–39, 60–64 | Day 2 |
| 4 | Sync and conflict resolution | 45–58 | Day 2–3 |
| 5 | Plugin interface | 64–72 | Day 3 |
| 6 | Prime, update, polish | 59–63, 76–79, protected main | Day 3–4 |

Four days to a working system. The sync/conflict phase is the hardest and most likely to slip.
