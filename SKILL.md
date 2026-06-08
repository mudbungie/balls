# balls — Agent Skill Guide

You are using **balls** (`bl`), a git-native task tracker for parallel agent
workflows. A task is a markdown file (`tasks/<id>.md`: TOML frontmatter + a
free-form body). State rides **two git branches** — `balls/config` (the landing,
holding `config/`) and a store branch (default `balls/tasks`, holding `tasks/`).
Git provides sync; there is no server.

## The default flow: finish your own task

**One agent takes a task all the way through: `claim → work → close → done`.**
There is no `review` step and no separate reviewer — claiming gives you a code
worktree, and `bl close` delivers it (squashes your work to `main`) and tears the
worktree down in one move. Do not stop after the work is written; an agent that
claims and walks away has not finished its job.

If you want a split submit/approve flow, wire a `close.pre` approval gate
explicitly. The default is solo: the agent that claims also closes.

## The worktree is the unit of work

`bl` tracks the code change a task delivers, not just the task. **`bl claim`
prints a path** — a git worktree on a `work/<id>` branch off `main`. While you
hold the claim, **all edits go in that worktree**, never on `main` directly.
Editing `main` outside the worktree bypasses the lifecycle: `bl close`'s delivery
squash captures the worktree's diff, so a stray `main` edit is invisible to it —
the task closes cleanly while leaving your change behind, undelivered.

Find your worktree path again any time with `bl show <id> --json` (the
`delivery-worktree` field) or `git worktree list`.

## State lives outside the repo (XDG)

balls does **not** keep its checkouts in your project repo. Per invocation path,
the landing and store live under
`$XDG_STATE_HOME/balls/clones/<percent-encoded-path>/` as `config/` (the landing)
and `tasks/` (the store). You rarely touch these directly — the verbs read and
write them for you — but that is where `git log`/`git show` of task history
lives. Your `work/<id>` code worktree lives in the delivery plugin's territory,
`$XDG_STATE_HOME/balls/plugins/<delivery>/<project-path>/<id>/` — the project
path **mirrored** (not percent-encoded) so it carries no `%`, which would break
`cargo`/`rust-lld` linking in that build dir. Find it with `bl show <id> --json`
(the `delivery-worktree` field) or `git worktree list`.

## Session start

Run `bl prime` at the start of every session:

```
bl prime --as YOUR_IDENTITY --json
```

`prime` is idempotent: on first run it **founds** the local substrate (there is
no separate `bl init`) — seeding `config/` from the install defaults and creating
the store — then syncs with the remote. It returns:

- **claimed**: tasks you already own — resume in their worktrees.
- **ready**: open, unblocked, unclaimed tasks, highest priority first.

To point a fresh checkout at a shared project, pass the remote once:
`bl prime --as ID --remote <git-url>` (or `--install <git-url>` to also adopt
that center's `config/`). Re-running plain `bl prime` later converges to a no-op.

## Identity

Every claim/close/prime is stamped with a worker identity from `--as ID`, else
`$BALLS_IDENTITY`, else `$USER`. Don't let an LLM invent its own name — models
collapse to the same few names across sessions and step on each other's claims.
Have the harness pick a name at session start and pass it as `--as` /
`$BALLS_IDENTITY`.

## Commands

| Command | What it does |
|---------|-------------|
| `bl prime [--as ID] [--remote URL] [--install URL] [--json]` | Sync + show ready/claimed. Founds the substrate on first run. Run at session start. |
| `bl sync [BRANCH] [--as ID]` | Pull the store from the remote (fetch + fast-forward). No arg syncs the configured store branch. |
| `bl list [--status ready\|blocked\|claimed] [--closed] [--all] [--tag T] [--json]` | List tasks. Default = live (non-closed). `--closed`/`--all` reconstruct archived tasks from history. |
| `bl show <id> [--json] [--verbose]` | Task detail. A closed id still resolves (reconstructed from history). |
| `bl dep-tree [--json]` | Parent/child tree with blocker/gate edges inline. |
| `bl create "TITLE" [--body B] [-p N] [-t TAG] [--parent ID] [--needs ID[:OP]] [--blocks OP\|ID:OP] [--as ID]` | File a task. Prints the new id. |
| `bl claim <id> [--as ID]` | Start work: materialize the `work/<id>` worktree, take occupancy. |
| `bl unclaim <id> [--as ID]` | Release a claim, remove the worktree. |
| `bl update <id> [--body B] [-p N] [-t TAG] [--parent ID] [--needs ...] [--blocks ...] [key=value]` | Edit fields. A bare `key=value` sets a preserved extra field. |
| `bl close <id> [-m MSG] [--as ID]` | Deliver (squash `work/<id>` to `main`) + archive the task + tear down the worktree. **Run from the repo root, not inside the worktree.** |
| `bl drop <id> [--as ID]` | Abandon a claim/task without delivering. |
| `bl skill` | Print this guide. |

> **For agents:** the human-facing output of `list`/`show`/`dep-tree` uses status
> glyphs and color on a tty. Always prefer `--json` for parsing — it is the
> **bedrock** projection (raw stored frontmatter, literal integer timestamps, no
> derived fields), the supported machine contract.

## Status is derived, never stored

A task has no `status` field. The three live states are computed on read:

- **claimed** — someone holds it (the `claimant` field is set).
- **blocked** — unclaimed, but an unresolved `claim`-blocker remains.
- **ready** — unclaimed with every `claim`-blocker resolved; claimable now.

A closed or dropped task has **no file** (absence = resolved). Its history —
including the delivery commit on `main` tagged `[bl-xxxx]` — is the record.

## Blockers and the dependency model

The one relational primitive is a blocker edge `{id, on}` on the *blocked* task:
"this task can't do op `on` until task `id` resolves." `on` is ANY op, but two
have create-time sugar:

- `--needs B[:OP]` — add a blocker on this task (default `OP = claim`, i.e. a
  dependency: can't be claimed until B closes).
- `--blocks OP` / `--blocks ID:OP` — the reciprocal: gate ANOTHER task's op on
  this one. `--parent X --blocks close` is a gate (X can't close until this does).

`--parent` is **containment only** — it builds the display tree and gates
nothing. An "epic" is just a task with children; to make a parent wait on its
children, add explicit `--needs`/`--blocks` edges. Core enforces blockers: a
`claim` of a blocked task or a `close` with an open gate is refused, naming the
blocker.

## Plugins

Behavior beyond the base (commit task files) is plugins — subprocesses wired in
`config/plugins.toml` under `[hooks]` (`<op>.<phase>` → an ordered list of plugin
names). Two ship by default:

- **tracker** — the only component that talks to a remote: fetch + ff on sync,
  push after each op, found/adopt on prime.
- **bl-delivery** — owns the `work/<id>` code worktree: materialize on claim,
  squash-deliver + tear down on close.

A plugin whose binary is not installed beside `bl` is pruned at prime, so a
remote-less or plugin-less box still works.

## Operating against a bare project repo

A common deployment is a **bare** project repo (no working tree at the root).
Then:

- `git status` at the bare root is fatal by design (`must be run in a work
  tree`), not a broken repo. For task state use `bl list`; for code state run
  `git status`/`git diff` inside your `work/<id>` worktree.
- All `bl` verbs run from the bare root. `bl close` does too — and **must not be
  run from inside the worktree it deletes**: `cd` to the repo root first, or your
  shell's working directory is removed underneath you.

## Removing or abandoning tasks

- A task you decided against (dupe, stale): `bl drop <id>` if you hold it, else
  `bl close <id>` to archive without delivering code.
- `update` only **adds** (`-t`/`--needs`/`--blocks`) or **sets**
  (`--body`/`-p`/`--parent`/`key=value`) — there is no remove flag. To clear a
  field, edit the store's `tasks/<id>.md`, commit on the store branch, and push
  (or let the next op's tracker push it).
