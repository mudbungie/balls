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

If you want a split submit/approve flow, add a review gate as an ordinary
close-blocker subtask (`bl create "review X" --parent X --blocks close`, or a
forge plugin that mints one at claim). Submission is git-native — push the work
branch and open the PR yourself, with the `[bl-id]` tag in the PR title so the
merge is recognized as the delivery. The default is solo: the agent that claims
also closes.

**Close is gated by the project's own pre-commit hook.** Delivery first folds
`main` into your work branch (so what gets checked is what actually lands, even
if `main` moved while you worked), then runs the repo's `pre-commit` hook on the
result and aborts the close if it fails — the task stays claimed and the
worktree stays up for the fix. A repo with no executable `pre-commit` hook is
ungated: close delivers unchecked. A merge conflict with `main` also aborts the
close (cleanly — no half-merge is left behind); merge `main` into the worktree
by hand, resolve, and close again.

Because close re-gates the **folded** tree — the tree that actually lands —
intermediate commits in the `work/<id>` worktree may reasonably use `git commit
--no-verify`: the gate-of-record runs at close, before anything lands, and is
never skipped. Whether each worktree commit also pays the hook (which may be
expensive) stays the repo's own policy decision; bl takes no position beyond
where the gate-of-record sits.

## The worktree is the unit of work

`bl` tracks the code change a task delivers, not just the task. `bl claim`
materializes a git worktree on a `work/<id>` branch off `main`. While you hold
the claim, **all edits go in that worktree**, never on `main` directly. Editing
`main` outside the worktree bypasses the lifecycle: `bl close`'s delivery squash
captures the worktree's diff, so a stray `main` edit is invisible to it — the
task closes cleanly while leaving your change behind, undelivered.

`bl claim` prints the worktree path to **stdout** — the verb's one product, the
way `create` prints the id — and `bl prime` re-prints the path of every task you
still hold. `bl show <id>` (human view) also folds a `worktree` line in when the
worktree exists on this machine. The path is computed, never stored: `bl show
--json` stays the lossless mirror of stored frontmatter and never carries it (it
is machine-local). `git worktree list` (the `work/<id>` line) is the git-side read.

## State lives outside the repo (XDG)

balls does **not** keep its checkouts in your project repo. Per invocation path,
the landing and store live under
`$XDG_STATE_HOME/balls/clones/<percent-encoded-path>/` as `config/` (the landing)
and `tasks/` (the store). You rarely touch these directly — the verbs read and
write them for you — but that is where `git log`/`git show` of task history
lives. Your `work/<id>` code worktree lives in the delivery plugin's territory,
`$XDG_STATE_HOME/balls/plugins/<delivery>/<project-path>/<id>/` — the project
path **mirrored** (not percent-encoded) so it carries no `%`, which would break
`cargo`/`rust-lld` linking in that build dir. `bl claim`/`bl prime` print it;
`bl show <id>` (human view) and `git worktree list` read it back.

## Session start

Run `bl prime` at the start of every session:

```
bl prime --as YOUR_IDENTITY
```

`prime` is idempotent: on first run it **founds** the local substrate (there is
no separate `bl init`) — seeding `config/` from the install defaults and creating
the store — then syncs with the remote and re-materializes the worktrees of any
tasks you still hold. It also prunes the settled `work/<id>` branches that
delivered closes leave behind (a branch carrying committed, undelivered work —
e.g. after an unclaim — is kept; a later claim + close delivers it). It prints
no listing of its own; once primed, read the two sets you care about with
`bl list` (the single listing verb):

- **ready** (open, unblocked, unclaimed, highest priority first): `bl list`, or
  `bl list -s ready` for that rung alone.
- **claimed** (tasks you already own — resume in their worktrees): `bl list -s claimed`.

The store remote resolves the same way on **every** command: `--remote URL` (a
per-op override — it is **not** remembered) > the per-machine `task-remote`
(`bl conf set task-remote <url>`) > the project repo's `origin`. A fresh clone
whose `origin` carries the store just works: `bl prime; bl list`. To point a
checkout with no such `origin` at a shared project, set a durable pointer —
`git remote add origin <hub>` or `bl conf set task-remote <hub>` — then
`bl prime` (add `--install <hub>` to also adopt that center's `config/`).
`--remote` alone shapes only that one invocation; prime warns when nothing
durable backs it, because every later plain command would silently run
stealth. Re-running plain `bl prime` converges to a no-op, and `bl conf` shows
the remote/branch a checkout actually resolves.

In a repo with a pushable `origin`, prime founds a `balls/tasks` branch there
and pushes it. `bl prime --stealth` is the opt-out: the store stays local and
prime founds, pushes, and discovers nothing. It contradicts
`--remote`/`--center`/`--install` (each names a remote), refused at parse.

## Local config (`bl conf`)

`bl conf` dumps every resolved config value, the layer it came from
(`cli`/`xdg`/`landing`/`origin`/`default`), and the paths of the files behind
them; `bl conf <key>` prints one value (stdout) with its provenance (stderr).
A checkout with no durable remote shows `task-remote (none)` — that checkout
is stealth. Writes are scope-keyed — the key implies the file, there is no
`--scope` flag:

- `bl conf set task-remote <url>` — per-machine store remote (XDG config).
- `bl conf set task-branch <name>` / `bl conf set log-level <level>` — landing
  `balls.toml`, committed on `balls/config`. Re-pointing `task-branch` strands
  the store unless you move it first (see the spec's re-home discipline).
- `bl conf set|append|prepend|remove <op>.<pre|post> <name...>` — the
  `[hooks]` plugin schedule (`show`/`list` are bare keys: `bl conf append list
  <name>`). `set` replaces the whole list; `append`/`prepend`/`remove` compose
  one name and converge (a present name re-appended, or an absent one removed,
  is a no-op). Naming a plugin whose binary isn't installed beside `bl` leaves
  a dangling entry — pruned at seed, a clean error at dispatch — never code
  execution; `conf` writes the schedule, never a binary.

`conf` is local-only: it never crosses a checkout boundary (adopting another
checkout's config is `install`'s consent-gated job) and runs no plugins —
config never syncs.

## Identity

Every claim/close/prime is stamped with a worker identity from `--as ID`, else
`$USER`, else the literal `"unknown"`. Don't let an LLM invent its own name —
models collapse to the same few names across sessions and step on each other's
claims. Have the harness pick a name at session start and pass it as `--as`.

## Commands

| Command | What it does |
|---------|-------------|
| `bl prime [--as ID] [--remote URL] [--install URL] [--stealth]` | Found the substrate (first run) + sync + re-materialize the worktrees of tasks you still hold (prints their paths). Prints no listing of its own. `--stealth` opts out of any store remote (store stays local). Run at session start, then `bl list`. |
| `bl sync [BRANCH] [--as ID] [--remote URL]` | Pull the store from the remote (fetch + fast-forward; the remote resolves `--remote` > `task-remote` > `origin`, like every op). No arg syncs the configured store branch. |
| `bl conf [<key>]` / `bl conf set\|append\|prepend\|remove <key> <value...>` | Local config CRUD. No args: dump every resolved value + source layer + file paths. Keys: `task-remote` (per-machine XDG), `task-branch`/`log-level` (landing), `<op>.<pre\|post>`/`show`/`list` (the `[hooks]` schedule). Local-only: never crosses a checkout, never touches a plugin binary. |
| `bl install [PATH] --from REF [--to REF] [--as ID]` | Copy a committed PATH between branches, sealed as one commit on `--to`'s tip (§6 capability transfer). Shape decides: folder = mirror (deletions propagate!), file/glob = additive union; `bin/` never travels. Defaults: PATH `config`, `--to` the landing. Prints `N added / M deleted`. |
| `bl list [-s\|--status ready\|blocked\|claimed\|closed] [--all] [--tag T] [--json]` | List tasks. Default = live (non-closed). `-s closed` (or `--all` for live+dead) reconstructs archived tasks from history. |
| `bl show <id> [--json]` | Task detail (always full: fields, blockers, children, body). A closed id still resolves (reconstructed from history). |
| `bl create "TITLE" [--body B] [-p N] [-t TAG] [--parent ID] [--subtask-of ID] [--needs ID[:OP]] [--blocks OP\|ID:OP] [-m MSG] [--as ID] [-- TITLE]` | File a task (`--body` sets the markdown body; `-m` the commit note). Prints the new id. A `--` ends option parsing (getopt; create and update alike), so an untrusted `-`-leading title can't hijack a flag: `bl create -- "$TITLE"`. |
| `bl claim <id> [--as ID]` | Start work: materialize the `work/<id>` worktree (prints its path), take occupancy. |
| `bl unclaim <id> [--as ID]` | Release a claim, remove the worktree. |
| `bl update <id> [--edit] [--title T] [--body B] [--parent ID\|--no-parent] [-p N\|--no-priority] [-t TAG] [--no-tag TAG] [--needs ID[:OP]] [--no-needs ID] [key=value] [-m MSG]` | Overwrite **any** field: `--title`/`--body`, set or clear the `--parent`/`-p` scalar, add (`-t`) or drop (`--no-tag`) a tag, set (`key=value`) or remove (`key=`) a preserved extra, add (`--needs`) or unlink (`--no-needs`) one of this task's own blockers. Only reciprocal `--blocks` (an edge on ANOTHER task) stays **create-only**. `-m` is the commit note. `--edit` (human-only) sources the whole change from `$EDITOR` instead — see below. |
| `bl close <id> [-m MSG] [--as ID]` | Deliver (fold `main` in, run the repo's pre-commit hook — a failure aborts the close — then squash `work/<id>` to `main`) + archive the task + tear down the worktree. |
| `bl skill` | Print this guide (the full manual). |
| `bl help` | Print the terse command directory (also `--help`/`-h`). |

> **For agents:** the human-facing output of `list`/`show` uses status
> glyphs and color on a tty. Always prefer `--json` for parsing — it is the
> **bedrock** projection (raw stored frontmatter, literal integer timestamps, no
> derived fields), the supported machine contract.
>
> **Output streams:** stdout carries a verb's one product and nothing else:
> `create` prints the minted id (so `id=$(bl create "…")` captures it clean),
> `claim` prints the worktree path, and `prime` prints the path of each
> still-held task's worktree. Every other mutating verb
> (`unclaim`/`update`/`close`) prints nothing to stdout. The terse
> confirmations and the op log (JSON lines) are on **stderr** — for clean
> scripting without losing the confirmations, silence the op log with the global
> `bl --log-level error <verb>` (levels `debug`/`info`/`error`; there is no
> `warn` — an unrecognised level is a usage error naming the ladder) instead of
> redirecting `2>/dev/null`.

## Status is derived, never stored

A task has no `status` field. The three live states are computed on read:

- **claimed** — someone holds it (the `claimant` field is set).
- **blocked** — unclaimed, but an unresolved `claim`-blocker remains.
- **ready** — unclaimed with every `claim`-blocker resolved; claimable now.

A closed task has **no file** (absence = resolved). Its history — including the
delivery commit on `main` tagged `[bl-xxxx]` — is the record. Closing is the
ONLY retirement: to abandon a held task, `bl unclaim` then `bl close` (an empty
worktree delivers nothing), so a `close`-gate guards every way a task can die.

## Blockers and the dependency model

The one relational primitive is a blocker edge `{id, on}` on the *blocked* task:
"this task can't do op `on` until task `id` resolves." `on` is ANY op, but two
have create-time sugar:

- `--needs B[:OP]` — add a blocker on this task (default `OP = claim`, i.e. a
  dependency: can't be claimed until B closes).
- `--blocks OP` / `--blocks ID:OP` — the reciprocal: gate ANOTHER task's op on
  this one. `--parent X --blocks close` is a gate (X can't close until this does).
- `--subtask-of E` — **the everyday subtask spelling**: `--parent E --blocks
  close` in one word (child of E, and E can't close until it does). Prefer this
  over bare `--parent` when filing subtasks — the gate rides in the flag's name,
  so it can't be silently forgotten. Mutually exclusive with `--parent`;
  create-only.

`--parent` is **containment only** — it builds the display tree and gates
nothing. An "epic" is just a task with children; to make a parent wait on its
children, add explicit edges (`--subtask-of` at create is the usual way). Core
enforces blockers: a `claim` of a blocked task or a `close` with an open gate is
refused, naming the blocker. Closing a task that still has live children prints
a notice ("closed with N open children, none gating") — informational, never a
block: the children survive with dangling, display-only parent pointers.

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
- All `bl` verbs run from the bare root.

## Removing or abandoning tasks

- A task you decided against (dupe, stale): `bl close <id>` archives it; an
  empty deliverable delivers no code. If you hold it, `bl unclaim <id>` first —
  that tears down the worktree (uncommitted work dies with it; work COMMITTED on
  `work/<id>` survives and a later close delivers it — discard that explicitly
  with `git branch -D work/<id>`).
- `update` overwrites **every** ball field — there is no create-only split.
  `--title`/`--body` retitle and rewrite the markdown body; `--parent`/`-p` set a
  scalar and `--no-parent`/`--no-priority` clear it; `-t`/`--no-tag` add or drop a
  tag; `key=value`/`key=` set or remove a preserved extra; `--needs`/`--no-needs`
  add or unlink one of this task's own blockers (the §10 in-band fix for a
  mis-wired or cyclic blocker). The lone create-only flag is reciprocal `--blocks`
  (an edge naming this task on ANOTHER), since that is not this task's own field.
- `bl update <id> --edit` is the **human projection** of the same update: it
  opens the stored `tasks/<id>.md` (frontmatter + body) in `$EDITOR` (else
  `$VISUAL`), blocking, then runs the saved buffer through the normal update
  seal. It is mutually exclusive with the field flags and `key=value` extras
  (they would race over the payload) — set fields OR hand-edit. A non-tty stdin
  or an unset editor is an **error**, so agents keep using flag-driven update.
  The buffer is parse-validated on save (bad TOML / a missing required field is
  rejected with the parse error, then re-edit or abort — garbage is never
  committed); an unchanged buffer is a no-op. The id is the path and `created`
  is history, so neither is hand-editable; `updated` is always restamped by the
  seal.
