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
```

This builds a release binary, installs `bl` to `~/.local/bin/`, and installs shell completions to `~/.local/share/`. Make sure `~/.local/bin` is on your `PATH`. To remove everything `make install` placed:

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

## Principles

1. **Git is the database.** Task files are committed, pushed, pulled, and merged like code. No external storage engine.
2. **One file per task.** Atomic unit of state. Git merge conflicts are per-task, never global.
3. **Derived state is computed, never stored.** Completion percentages, ready queues, dependency trees — all calculated at read time from task files.
4. **Local cache is disposable.** The `.balls/local/` directory is gitignored ephemeral state. Deleting it loses nothing durable.
5. **Offline-safe.** All operations produce valid local state. Conflicts are resolved at merge time, never prevented by connectivity checks.
6. **Worktrees are first-class.** Claiming a task creates a worktree. The worktree name is the task ID. One task, one workspace.
7. **The CLI is a convenience, not a requirement.** Every operation is expressible as file edits + git commands. A human with vim, ln, and git can do everything `bl` does.
8. **Plugins extend, core stays small.** External integrations (Jira, Linear, GitHub Issues) are handled by a plugin interface. Auth, sync logic, and API specifics never enter the core.

---

## Glossary

| Term | Meaning |
|---|---|
| **task** | A unit of work. One JSON file in `.balls/tasks/`. |
| **ready** | A task that is open, has all dependencies met, and is unclaimed. |
| **claim** | Taking ownership of a task. Creates a worktree. |
| **review** | Submitting work for approval. Merges to main, keeps worktree for potential rework. |
| **close** | Approving completed work. Archives the task file, removes worktree. |
| **drop** | Releasing a claim. Destroys the worktree, resets the task to open. |
| **sync** | Fetch + merge + push against the git remote. |
| **plugin** | An external executable that implements the plugin interface for a specific integration (e.g., Jira). |

---

## File and Folder Layout

```
project/
├── .balls/                       # root of task tracking
│   ├── tasks/                   # git-tracked task files
│   │   ├── bl-a1b2.json
│   │   ├── bl-c3d4.json
│   │   └── ...
│   ├── config.json              # git-tracked project config
│   ├── plugins/                 # git-tracked plugin configs
│   │   └── jira.json            # per-plugin config (urls, project keys, field maps)
│   └── local/                   # gitignored ephemeral state
│       ├── claims/              # one file per active local claim
│       ├── lock/                # flock files for local atomic operations
│       └── plugins/             # plugin runtime state (tokens, caches)
├── .balls-worktrees/             # gitignored, worktree checkouts
│   ├── bl-a1b2/                 # worktree for task bl-a1b2
│   └── bl-c3d4/                 # worktree for task bl-c3d4
└── ... (project files)
```

### .gitignore entries

```
.balls/local/
.balls-worktrees/
```

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
  "notes": [],
  "closed_children": [],
  "external": {}
}
```

### Field definitions

| Field | Type | Description |
|---|---|---|
| `id` | string | Format `bl-XXXX` (4 hex chars). Generated from sha1 of title + timestamp, truncated. |
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
| `notes` | object[] | Append-only log: `{"ts": "...", "author": "...", "text": "..."}` |
| `closed_children` | object[] | Archived child tasks: `{"id": "...", "title": "...", "closed_at": "..."}`. Populated when a child task is closed and archived. |
| `external` | object | Plugin-managed foreign keys. e.g., `{"jira": {"key": "PROJ-123", "synced_at": "..."}}`. Core never reads this; plugins own it. |

### ID generation

```
echo -n "${title}${timestamp}" | sha1sum | cut -c1-4 | sed 's/^/bl-/'
```

On collision (file already exists), increment timestamp and retry.

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

When a task is closed via `bl close`, the task file is deleted from `.balls/tasks/` after the close commit. The full task data is preserved in git history. If the task has a parent, the parent's `closed_children` array is updated with the archived child's ID, title, and close timestamp. This keeps the working set small — only live tasks exist in HEAD.

---

## Local Cache (.balls/local/)

### flock

`flock` is a Linux utility (part of `util-linux`, present on all Ubuntu/Debian/RHEL systems) that provides advisory file locking. It uses a lock file and the `flock(2)` syscall to ensure only one process runs a critical section at a time:

```bash
flock .balls/local/lock/bl-a1b2.lock -c 'update-task-and-commit'
```

If another process holds the lock, the caller blocks until it's released. No polling, no races.

### claims/

One file per claimed task. Filename is the task ID. Contents:

```
worker=dev1/agent-alpha
pid=48291
claimed_at=2026-04-09T15:00:00Z
```

This is a performance shortcut for fast local double-claim prevention. The source of truth is `claimed_by` in the git-tracked task file.

### lock/

One lock file per task ID during write operations. Used via `flock` to serialize concurrent local writes to the same task.

---

## Worktree Lifecycle

### Creation (on claim)

```bash
# 1. Acquire local lock
flock .balls/local/lock/bl-a1b2.lock -c '

# 2. Check not already claimed (local cache)
[ ! -f .balls/local/claims/bl-a1b2 ] || exit 1

# 3. Update task file
#    Set claimed_by, status -> in_progress, branch -> work/bl-a1b2
tmp=$(mktemp)
jq ".claimed_by = \"agent-alpha\" | .status = \"in_progress\" | .branch = \"work/bl-a1b2\"" \
  .balls/tasks/bl-a1b2.json > "$tmp"
mv "$tmp" .balls/tasks/bl-a1b2.json

# 4. Commit the claim
git add .balls/tasks/bl-a1b2.json
git commit -m "balls: claim bl-a1b2"

# 5. Create worktree
git worktree add .balls-worktrees/bl-a1b2 -b work/bl-a1b2

# 6. Symlink local cache into worktree
ln -s "$(pwd)/.balls/local" ".balls-worktrees/bl-a1b2/.balls/local"

# 7. Write local claim file
cat > .balls/local/claims/bl-a1b2 <<EOF
worker=agent-alpha
pid=$$
claimed_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
EOF
'
```

### Submit for review (agent finishes work)

```bash
# 1. Commit all work in worktree
cd .balls-worktrees/bl-a1b2
git add -A && git commit -m "balls: work on bl-a1b2"

# 2. Merge main into worktree (forward merge, catches up)
git merge main

# 3. Set task status to review
jq '.status = "review"' .balls/tasks/bl-a1b2.json > tmp && mv tmp .balls/tasks/bl-a1b2.json
git add -A && git commit -m "balls: review bl-a1b2"

# 4. Merge worktree into main (--no-ff: preserves branch topology)
cd ../..
git merge --no-ff work/bl-a1b2 -m "balls: merge bl-a1b2"
```

The worktree stays. The agent's CWD is never destroyed.

### Close (reviewer approves)

```bash
# Run from the repo root, never from inside the worktree.
# 1. Archive the task file (delete from HEAD, preserved in git history)
git rm .balls/tasks/bl-a1b2.json && git commit -m "balls: archive bl-a1b2"

# 2. Remove worktree and branch
git worktree remove .balls-worktrees/bl-a1b2
git branch -d work/bl-a1b2
rm -f .balls/local/claims/bl-a1b2
```

### Reject (reviewer requests rework)

```bash
# Set status back to in_progress. Agent resumes in existing worktree.
bl update bl-a1b2 status=in_progress --note "needs error handling"
# Agent's next `bl review` will merge main first, picking up this change.
```

---

## Conflict Resolution

### Rules

When merging task files that conflict:

1. **Status precedence:** `closed` > `review` > `in_progress` > `blocked` > `open` > `deferred`. Higher status wins.
2. **Notes:** Union by timestamp. Append-only, so both sides' notes are kept.
3. **Timestamps:** Later `updated_at` wins for all non-status fields.
4. **claimed_by:** If status resolves to `closed`, `claimed_by` comes from the closing side. Otherwise, first writer wins.

### Scenarios

**Same task claimed by two workers offline.** First push wins. Second worker's `bl sync` detects conflict, resets their claim, task file resolved per rules above. CLI suggests next ready task.

**Same task closed by two workers.** First merge wins. Second gets a conflict. CLI prompts: "Task already closed. File new task with your changes? [y/n]"

**One closes, one updates.** Closed wins. Update's notes are preserved in the merge.

**Different tasks edited concurrently.** No conflict. Different files, git merges cleanly.

---

## CLI Commands

### bl init [--stealth]

Creates `.balls/tasks/`, `.balls/local/`, `.balls/plugins/`, `.balls/config.json`. Adds gitignore entries. Commits. If `.balls/tasks/` already exists (cloned repo), creates only local dirs.

With `--stealth`, tasks are stored outside the repo (`~/.local/share/balls/<hash>/tasks/`) and are not git-tracked. All other operations (create, list, claim, close) work identically. Useful for local-only planning that shouldn't appear in PRs.

**By hand:** `mkdir -p .balls/{tasks,plugins,local/{claims,lock}}`, write `config.json`, append to `.gitignore`, commit.

### bl create TITLE [options]

```
bl create "Implement auth middleware" -p 1 -t task --parent bl-x9y8 --dep bl-c3d4 --tag auth
```

Generates ID, writes task file, commits. Rejects circular deps and nonexistent dep IDs. Triggers plugin sync if configured.

**By hand:** Write the JSON file, `git add`, `git commit`.

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

**By hand:** `cat .balls/tasks/*.json | jq 'select(.status != "closed")' | jq -s 'sort_by(.priority, .created_at)'`

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

Displays one task with computed fields (blocked status, children if parent, dependency chain).

**By hand:** `cat .balls/tasks/bl-a1b2.json | jq .`

### bl claim ID [--as IDENTITY]

```
bl claim bl-a1b2
bl claim bl-a1b2 --as dev1/agent-alpha
```

Validates task is claimable → updates task file → commits → creates worktree → symlinks local cache → writes local claim → prints worktree path. Triggers plugin sync if configured.

Fails if already claimed locally, deps unmet, or task not open.

**By hand:** See worktree creation section.

### bl review ID [--message MSG]

```
bl review bl-a1b2 --message "Ready for review"
```

Agent's safe exit point. Commits work → merges main into worktree → sets status=review → merges to main (--no-ff). Worktree and claim stay intact. The agent's CWD is never destroyed. Triggers plugin push if configured.

If the reviewer rejects (sets status back to `in_progress`), the agent resumes in the existing worktree and calls `bl review` again — it will merge main first, picking up the rejection.

**By hand:** See submit-for-review section.

### bl close ID [--message MSG]

```
bl close bl-a1b2 --message "Approved"
```

Reviewer/supervisor operation. Archives the task file (deletes from HEAD), removes worktree, cleans up claim and branch. **Rejects if run from inside the worktree** — must run from the repo root. Prints the repo root path on success. Triggers plugin push if configured.

**By hand:** See close section.

### bl update ID [field=value ...] [--note TEXT]

```
bl update bl-a1b2 priority=2
bl update bl-a1b2 status=blocked --note "Waiting on API team"
```

Edits task file fields, commits. Triggers plugin sync if configured.

**By hand:** Edit JSON, `git add`, `git commit`.

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
```

Adds a typed link. Link types: `relates_to`, `duplicates`, `supersedes`, `replies_to`. Validates target exists. Idempotent. Commits.

### bl link rm TASK TYPE TARGET

Removes a typed link. Commits.

### bl sync

```
bl sync
```

1. `git fetch origin`
2. Merge remote into current branch
3. Auto-resolve any `.balls/tasks/` conflicts per resolution rules
4. `git push origin`
5. Run plugin sync (if configured)

Fetch failure is not fatal. Push failure triggers fetch+merge+retry once.

**By hand:** `git fetch && git merge origin/main`, resolve task file conflicts per rules, `git push`.

### bl prime [--as IDENTITY]

Session bootstrap for agents. Runs `bl sync`, then outputs:
- Worker identity
- Ready tasks ranked by priority
- Currently claimed tasks for this identity (for session resume)

Designed to be injected into an agent's context at session start.

### bl resolve FILE

Manual conflict resolution helper. Parses both sides of a conflicted task file, applies resolution rules, writes result.

---

## Config (.balls/config.json)

Git-tracked, shared across team.

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

`tasks_dir` overrides the default tasks location (`.balls/tasks/`). Set automatically by `bl init --stealth` to an external path (`~/.local/share/balls/<repo-hash>/tasks/`). When tasks are external, they are not git-tracked.

---

## Plugin System

### Design

Plugins are external executables that implement a defined interface. Core knows how to call them but never contains integration-specific logic. Auth flows (Single Sign-On (SSO), Personal Access Tokens (PATs), OAuth, etc.) are entirely the plugin's responsibility, managed in `.balls/local/plugins/` where credentials and tokens live (gitignored, never committed).

### Interface

A plugin is an executable (any language) that responds to commands via argv:

```
balls-plugin-jira auth-setup --auth-dir .balls/local/plugins/jira/
balls-plugin-jira auth-check --auth-dir .balls/local/plugins/jira/
balls-plugin-jira push --task bl-a1b2 --config .balls/plugins/jira.json --auth-dir .balls/local/plugins/jira/
balls-plugin-jira sync --config .balls/plugins/jira.json --auth-dir .balls/local/plugins/jira/
balls-plugin-jira sync --task bl-a1b2 --config .balls/plugins/jira.json --auth-dir .balls/local/plugins/jira/
```

### Commands a plugin must implement

| Command | Input | Output | Description |
|---|---|---|---|
| `auth-setup` | (interactive) | Writes creds to `auth-dir` | One-time auth configuration. Handles SSO, PAT entry, OAuth flows — whatever the service needs. |
| `auth-check` | Reads `auth-dir` | Exit 0 if valid, 1 if expired/missing | Tests whether current credentials work. Core calls this before push/sync. |
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

The model is identical to single-machine. Each developer:

1. Clones the repo. Gets `.balls/tasks/` with full task state.
2. Runs `bl init` to create local ephemeral dirs.
3. Runs `bl sync` to stay current.
4. Claims tasks, works in worktrees, pushes.

A developer and their agents are just workers on the same machine sharing a local cache. Remote developers are workers on different machines sharing state through git. The coordination model is the same: optimistic concurrency, conflict at merge time, resolution per rules.

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

33. Close a task. Task file updated, all changes committed, merged to main, worktree removed, local claim cleaned.
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

54. Dev A creates tasks, pushes. Dev B clones, sees all tasks.
55. Dev A claims task, pushes. Dev B's `bl ready` does not show that task.
56. Multiple devs running agent swarms. Each agent claims distinct tasks. Git push serializes merges.
57. New dev joins, clones, runs `bl init`. Full task state available immediately.
58. Dev works offline for a day. Creates and closes tasks. Comes online, `bl sync` resolves conflicts.

### Agent Lifecycle

59. Agent starts, runs `bl prime`. Gets synced state, ready queue, any in-progress tasks for this identity.
60. Agent picks top ready task, claims it, works in worktree.
61. Agent finishes, runs `bl review`. Work merged to main, worktree stays, status=review.
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

Ball's thesis: every layer of infrastructure you add is a layer that can break, a layer to learn, a layer to operate. The best tool is the one with the fewest moving parts that solves the problem.

**The CLI is the agent interface.** Agents already have shell access. `bl ready --json` is a tool call. There is no need for MCP servers, REST APIs, or protocol adapters. If you can run a command, you can use balls.

**Git is the archive.** Closed tasks are deleted from HEAD and preserved in git history. There is no compaction, no garbage collection, no cleanup threshold. Only live tasks exist in the working set. Old tasks are retrievable via `git log` when needed. The working set stays small naturally.

**Git is the database.** Task files are committed, pushed, pulled, and merged like code. There is no second version-control system to reconcile, no schema migrations, no embedded database engine. If you understand git, you understand balls's storage model.

---

## Why Not Existing Alternatives

### Beads

Beads introduced the right insight: agents need structured, queryable, persistent task state — not markdown files. But the implementation chose Dolt (a version-controlled SQL database) as the storage backend. Dolt requires CGO, a C compiler, embedded database management, schema migrations, and a mental model separate from git. The Dolt branching model operates independently of git branches, creating two parallel version-control systems that developers must reconcile. For a tool whose primary job is tracking a few hundred tasks, this is a heavy foundation.

Beads also positions itself as agent-first, which led to design decisions (embedded Dolt for sub-millisecond queries, cell-level merge for concurrent agent writes) that optimize for a scenario that doesn't need optimizing. Reading a few hundred JSON files is already millisecond-fast. Git file-level merge is sufficient when each task is one file. The complexity bought marginal performance on a workload that was never slow.

Beads' compaction system summarizes old tasks to save context window space. Ball takes a simpler approach: closed tasks are deleted from HEAD entirely. Only live tasks exist. No compaction needed because there's nothing to compact.

### Cline Kanban

Cline Kanban provides a visual board for agent orchestration with worktree-per-task isolation. It solves the human attention problem well. But it's local-only with no multi-machine story, closed-source infrastructure, and tightly coupled to the Cline ecosystem despite claiming agent-agnosticism. There is no durable shared state — each developer's board is independent.

### GitHub Issues / Jira / Linear

Traditional trackers weren't designed for agent workflows. They require network round-trips for every read, can't be queried offline, don't support the claim-and-worktree lifecycle, and have no concept of local-first operation. They remain the right tools for human project management. Ball integrates with them via plugins rather than replacing them.

### The balls approach

Ball takes the core insight — structured task files, dependency tracking, agent-native CLI — and implements it on the only infrastructure every developer already has: git. Tasks are files. Sync is push/pull. History is git log. Collaboration is merge. There is nothing to install except a small CLI, nothing to configure except a JSON file, and nothing to operate except git.
