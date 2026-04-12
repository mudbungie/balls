# SPEC: Orphan-Branch Task State

Status: draft
Scope: changes how balls stores task state in a git repo. No user-facing command surface changes except `bl init`.

## 1. Motivation

Using balls correctly should produce a clean `git log main` — one commit per delivered task, carrying a real feature message and the `[bl-xxxx]` tag. Today the log contains, per task:

```
<feature commit> [bl-xxxx]
balls: close bl-xxxx - <title>
balls: claim bl-xxxx - <title>
balls: create bl-xxxx - <title>
```

Three of the four commits are bookkeeping for `.balls/tasks/*.json`, a set of tracked files in main. Every lifecycle transition mutates a tracked file, so every transition requires a commit. The squash-merge work in bl-c506 already collapsed the worktree-side noise (`work on`, `review`, `merge`, `archive`); this spec finishes the job by moving task state off of main entirely.

## 2. Principles

These are architectural invariants. Any implementation choice that violates one is out of scope; revisit the spec instead of compromising.

1. **Clean log.** `git log main` contains only substantive feature commits. No `balls: *` commits ever land on main.
2. **Git is the database.** Task state lives in git objects. There is no parallel event log, no sidecar database, no ledger-in-the-accounting-book sense. `git log balls/tasks` *is* the history of task state.
3. **Standard tools suffice.** Anything `bl` does to task state, a user with git, `jq`, and `$EDITOR` can do by hand. `bl` is a convenience wrapper over publishable shell commands, never a privileged gateway.
4. **Naïve visibility.** An engineer on main can `ls .balls/tasks/`, `cat` a task file, open it in an editor, and `grep -r` across all tasks with zero balls-specific knowledge.
5. **Text-mergeable state.** Concurrent edits to disjoint fields of the same task merge cleanly under stock `git merge`, with no custom merge driver.

## 3. Terminology

Fixed terms, used throughout this document and in code comments. Do not invent synonyms.

- **state branch**: the orphan branch `balls/tasks`. Contains task files and nothing else. Has no shared ancestry with main.
- **state worktree**: a git worktree at `.balls/worktree/` with the state branch checked out. This is where task files physically live.
- **task file**: `.balls/worktree/.balls/tasks/<id>.json`. The canonical JSON document describing one task.
- **notes file**: `.balls/worktree/.balls/tasks/<id>.notes.jsonl`. Append-only newline-delimited JSON of notes attached to one task.
- **task symlink**: `.balls/tasks`, a symlink pointing at `worktree/.balls/tasks`. Gives engineers on main a stable path to read/write task files with standard tools.
- **state history**: `git log balls/tasks`. The sequence of commits on the state branch; each commit records one or more state transitions.
- **delivery tag**: the `[bl-xxxx]` token embedded in a main-branch squash-merge commit message. Immutable, embedded, survives any rebase that doesn't manually rewrite the message.

The word "ledger" is prohibited. It implied a parallel record-keeping structure that does not exist.

## 4. Topology

```
main                       balls/tasks (orphan)
  |                              |
  feature commit [bl-0001]       state commit: create bl-0001
  feature commit [bl-0002]       state commit: claim bl-0001
  feature commit [bl-0003]       state commit: close bl-0001
                                 state commit: create bl-0002
                                 ...
```

The two histories are parallel and unrelated. No merge between them is ever legal or expected. `git merge balls/tasks` from main is refused by git without `--allow-unrelated-histories`; this is a feature.

Filesystem layout of a balls-enabled repo:

```
<repo>/
├── .git/
│   └── worktrees/worktree/        # git's admin dir for the state worktree
├── .balls/
│   ├── worktree/                  # state worktree (gitignored on main)
│   │   └── .balls/
│   │       ├── tasks/
│   │       │   ├── bl-0001.json
│   │       │   └── bl-0001.notes.jsonl
│   │       └── local/             # claims, locks; see §8
│   └── tasks → worktree/.balls/tasks   # symlink, gitignored on main
├── .gitignore                     # includes .balls/worktree, .balls/tasks
└── <rest of the project>
```

On main: `.balls/tasks` and `.balls/worktree` are gitignored; nothing under `.balls/` is tracked by main.

On the state branch: the tree contains `.balls/tasks/*.json` and `.balls/tasks/*.notes.jsonl` and nothing else. No source code, no config, no README. The state branch is a pure data branch.

## 5. Task file schema invariant

Task files must be text-mergeable. Git's default line-based three-way merge must produce a clean result whenever two workers edit disjoint fields of the same task. This is a hard constraint on the file format.

Requirements:

1. **One top-level field per line.** The task file is a JSON object whose serialization places each key on its own line, with a trailing comma (or no comma on the last line), no nested pretty-printed objects on multiple lines unless each nested line is self-contained.
2. **Stable key ordering.** Keys are serialized in a fixed order (alphabetical, or a predeclared schema order). Two writers producing the same logical state produce byte-identical files.
3. **Trailing newline.** Files always end with `\n`. Eliminates a common false conflict.
4. **No in-place growing arrays.** Arrays whose length changes (notes, dependency lists with frequent additions) must not live inside the task file. They are split into sibling files:
   - **Notes**: `<id>.notes.jsonl`, append-only, one JSON object per line. Concurrent appends merge cleanly because new lines always land at the end and git's merge handles disjoint line additions in the common case.
   - **Dependencies** are expected to change rarely and can stay inside the task file as a sorted array, one element per line if the array is non-trivial. If we find dep churn in practice, split them too.
5. **Scalar fields only at the top level.** Nested objects (e.g. `closed_children`) either live in their own file or are flattened into scalar keys.

Concurrent edits to the *same* field are a genuine conflict. The spec does not try to merge them. `bl sync` must detect the conflict and surface it to the user with a clear resolution path; see §7.

Schema changes that break mergeability are breaking changes and require a version bump.

## 6. Delivery link: tag is ground truth, SHA is a hint

Each task file carries a `delivered_in` field pointing at the squash commit on main that delivered the task. This is a performance hint, not a source of truth.

- **Ground truth**: the delivery tag `[bl-xxxx]` embedded in the commit message of the squash merge. It is discovered with `git log --grep='\[bl-xxxx\]' main`.
- **Hint**: the `delivered_in` SHA stored in the task file at close time. Used for O(1) lookup without scanning the log.
- **Verification**: on any read that consumes `delivered_in`, balls verifies the SHA still exists and that its commit message still contains the delivery tag. If either check fails, balls re-resolves from the tag and updates the hint.
- **Resilience**: rebase, amend, cherry-pick, filter-branch — all survive. The tag is embedded in the commit message and travels with the commit. A delivery is only lost if someone manually rewrites the tag out of the commit message, which is tantamount to "this task was never delivered."

Implication: the delivery tag is now a load-bearing convention. `bl review` must include it in every squash commit message, and any tooling that rewrites main commits must preserve it.

## 7. Sync protocol

`bl sync` (invoked explicitly, or implicitly at the start of `bl prime`, `bl claim`, `bl review`, `bl list`, etc.) reconciles local state with the remote state branch. It is the only code path that pushes or pulls the state branch.

### 7.1 Pull

```
git -C .balls/worktree fetch origin balls/tasks
git -C .balls/worktree merge --ff-only origin/balls/tasks
```

Fast-forward only. If the local state branch has diverged from the remote, sync falls through to the conflict path.

### 7.2 Divergence

If local and remote state branches both have new commits since their last common ancestor:

1. Attempt `git merge origin/balls/tasks` in the state worktree. The schema invariant (§5) makes this clean for disjoint-field edits.
2. If the merge produces conflicts, enumerate the conflicted task files and present them. Each conflict is surfaced as `task <id>: field <name> edited on both sides`.
3. `bl sync` does not auto-resolve conflicts. The user (or `bl sync --resolve ours|theirs|interactive`) resolves, then re-runs sync.
4. Once clean, the merge commit is pushed as in §7.3.

### 7.3 Push

Pushing a delivery touches two refs: `balls/tasks` (state transition to closed) and `main` (the squash commit). Git cannot push both atomically. The protocol:

1. Commit the state transition to the state branch first (local).
2. Commit the squash merge to main (local).
3. Push `balls/tasks` first.
4. Push `main` second.
5. If step 3 fails (remote state branch advanced), pull §7.1/§7.2, rebase the local state commit on top, retry from step 3.
6. If step 4 fails (remote main advanced), the state branch is ahead. This is a recoverable half-push; see §7.4.

### 7.4 Half-push recovery

A half-push is: state branch says `closed`, but no commit reachable from `main` carries the `[bl-xxxx]` delivery tag. `bl sync` detects this on read and takes one of two actions:

- **If the local worktree still has the squash commit ready to push**: retry the main push.
- **If the local state is lost** (e.g. a different machine): roll the state branch back by committing a new state transition that restores `status: review` with a `sync_note` explaining the rollback. Never force-push.

Either action produces a durable, git-visible record of the recovery. No silent mutation of history.

### 7.5 Concurrent claim

Two workers attempt to claim the same task. Local task-lock prevents same-machine races (as today). Cross-machine:

1. Worker A commits `claim` on the state branch, pushes, succeeds.
2. Worker B commits `claim` on the state branch, pushes, rejected because the remote advanced.
3. Worker B's `bl sync` pulls, sees `claimed_by` already set, surfaces "already claimed by A" and rolls back its own claim commit locally.
4. Worker B's worktree is torn down as if the claim had failed at step 1.

## 8. The `.balls/local/` directory

Claims files, task locks, and any other machine-local coordination state live in `.balls/worktree/.balls/local/`. This directory is **tracked on the state branch but gitignored inside the state worktree**, using a per-worktree exclude in `.git/worktrees/worktree/info/exclude`. It exists on the filesystem, it is readable by all balls processes on the machine, but it is never committed.

Rationale: claims and locks are ephemeral and machine-specific. Committing them to the state branch would cause spurious conflicts on every claim/release cycle. They share the state worktree's filesystem path so that the existing symlink-based worktree claim sharing keeps working.

## 9. Stealth mode carve-out

In stealth mode (current behavior when `.balls/` is outside the repo), none of this applies:

- No state branch is created.
- No state worktree, no symlink.
- Task files live outside the repo entirely, as today.
- The `stealth` field in `Store` continues to gate all state-branch operations.

Stealth mode is the Windows fallback until symlink support is added there, and remains available for users who do not want task state in their repo at all.

## 10. `bl init` responsibilities

`bl init` is mandatory per-clone. It is idempotent and self-healing: running it on a repo that is already set up verifies and repairs, rather than failing.

On a fresh clone, `bl init` performs:

1. Check whether `balls/tasks` exists as a local or remote branch.
   - If it exists on the remote, fetch it.
   - If it does not exist anywhere, create it as an orphan branch with an empty initial commit.
2. Ensure the state worktree exists at `.balls/worktree/` and is checked out on `balls/tasks`. Run `git worktree add .balls/worktree balls/tasks` if not.
3. Ensure `.balls/tasks` is a symlink pointing at `worktree/.balls/tasks`. Create or repair it if not.
4. Ensure `.gitignore` on main contains `.balls/tasks` and `.balls/worktree`. Add them if not, and commit that change (this is the one exception to "balls never commits to main" — it happens exactly once per repo).
5. Configure the per-worktree exclude for `.balls/local/`.

`bl init` does not run implicitly. Other commands that need the state worktree detect its absence and fail with a clear "run `bl init` first" message.

## 11. Hand-edit workflow

This section is the concrete answer to "what does a user do when `bl` is broken and they need to edit a task by hand?" If this sequence stops working, the implementation has drifted from the spec.

**Edit and publish a task change:**

```bash
$EDITOR .balls/tasks/bl-abc.json
git -C .balls/worktree add -- .balls/tasks/bl-abc.json
git -C .balls/worktree commit -m "bl-abc: bump priority"
git -C .balls/worktree push origin balls/tasks
```

**Create a new task by hand:**

```bash
cat > .balls/tasks/bl-xxxx.json <<'JSON'
{
  "id": "bl-xxxx",
  "title": "Fix the auth timeout",
  ...
}
JSON
git -C .balls/worktree add -- .balls/tasks/bl-xxxx.json
git -C .balls/worktree commit -m "bl-xxxx: create"
git -C .balls/worktree push origin balls/tasks
```

**Pull remote state branch updates:**

```bash
git -C .balls/worktree pull --ff-only origin balls/tasks
```

**Browse history of a task:**

```bash
git -C .balls/worktree log -- .balls/tasks/bl-abc.json
git -C .balls/worktree show <sha>:.balls/tasks/bl-abc.json
```

**Find the commit on main that delivered a task:**

```bash
git log --grep='\[bl-abc\]' main
```

These commands must remain valid for the life of the design. If a future change breaks any of them, it is a breaking change to the spec.

## 12. Non-goals

- **No task-state search server, no API, no daemon.** Reads go through git objects. If that becomes slow enough to matter, the answer is to cache inside `bl`, not to move the canonical store.
- **No custom git merge driver.** The schema invariant (§5) is designed so stock git merge suffices. A custom driver would be a lower bound on the tool-coupling we're trying to avoid.
- **No cross-task transactional writes.** Each state-branch commit can touch one or more task files, but there is no higher-level transaction spanning multiple commits. If you need to see two changes together, put them in one commit.
- **No guarantee that `main` and `balls/tasks` stay in lockstep.** Half-pushes are expected and recoverable (§7.4). A consumer that needs strong consistency should use the delivery tag on main as the source of truth for "this task was delivered," not the state branch.

## 13. Open questions

These are explicitly unresolved. Resolve them in follow-up tasks, not by ad-hoc decisions during implementation.

1. **State worktree location: `.balls/worktree/` vs. `.balls/state/` vs. something else.** Current proposal is `.balls/worktree/` because it says what it is to a git-literate reader. Reconsider if a naming review finds a clearer term.
2. **Dependency array splitting.** Keep deps inside the task file until real dep churn proves it matters. If we see conflicts, split to `<id>.deps.jsonl`.
3. **Conflict resolution UX.** `bl sync --resolve ours|theirs|interactive` is a placeholder. The actual flag set needs a short round of design once real conflicts show up in tests.
4. **Pre-existing repos.** How does a repo that already has `.balls/tasks/` tracked on main migrate to the new topology? Probably: `bl migrate` stages a one-shot commit on main that removes the tracked files, then runs `bl init` to set up the state branch and replays the last-known state into it. Needs its own spec section before implementation.

## 14. Conformance tests

These are the tests the integration suite must have before any implementation code changes. Each bullet corresponds to at least one test.

1. Fresh clone → `bl init` → `bl list` reflects the state branch.
2. Two workers create different tasks → merge on state branch is clean.
3. Two workers edit disjoint fields of the same task → merge is clean (schema invariant test).
4. Two workers edit the same field of the same task → surfaced as a conflict with per-field diagnostics.
5. Two workers append notes to the same task → merge is clean (notes file append-only property).
6. Concurrent claim on the same task → one worker wins, the other is rolled back with a clear error.
7. Rebase main after delivery → `bl show <id>` still resolves to the delivering commit (tag-as-ground-truth test).
8. Half-push simulation: state-branch push succeeds, main push fails → next `bl sync` detects and surfaces the recovery path.
9. Hand-edit a task file through the symlink → publish via the §11 shell sequence → other workers see the change after sync.
10. `ls .balls/tasks/` and `jq . .balls/tasks/*.json` on main work without any `bl` command having been run since checkout (naïve visibility test; requires that `bl init` leaves the symlink and worktree persistent).
11. `git log main` over a full task lifecycle contains exactly one commit per delivered task, and that commit's message contains the delivery tag.
12. Stealth mode: `bl init --stealth` creates no state branch, no state worktree, no symlink, and the rest of balls behaves as today.
13. `.balls/local/` files (claims, locks) never appear in state-branch commits.
14. `bl init` run twice is a no-op on the second run.
15. `bl init` run against a repo with a damaged symlink or missing worktree repairs it.

No implementation change lands until every test in this list exists and fails for the right reason.
