# balls — Agent Operating Guide

You are using **balls** (`bl`), a git-native task tracker for parallel agent
workflows. This is the high-level guide: what balls is, where its state lives,
the invariants you must not violate, and the map of commands. **Each command
carries its own full usage under `bl <command> --skill`** — reach for that when
you are about to run one. This guide is deliberately short; the depth is one
level down.

A task is a markdown file (`tasks/<id>.md`: TOML frontmatter + a free-form
body). State rides **two git branches** — `balls/config` (the landing, holding
`config/`) and a store branch (default `balls/tasks`, holding `tasks/`). Git
provides sync; there is no server.

## The default flow: finish your own task

**One agent takes a task all the way through: `claim → work → close → done`.**
There is no `review` step and no separate reviewer — `bl claim` gives you a code
worktree, and `bl close` delivers it (squashes your work to `main`) and tears
the worktree down in one move. Do not stop after the work is written; an agent
that claims and walks away has not finished its job.

Session start is always `bl prime --as YOUR_IDENTITY`, then `bl list`.

## Where everything lives

It is all **simple git and worktrees in XDG folders** — no hidden database.

- **The landing and store** (task state) live per invocation path under
  `$XDG_STATE_HOME/balls/clones/<percent-encoded-path>/` as `config/` (the
  landing) and `tasks/` (the store). The verbs read and write them for you; that
  is where `git log`/`git show` of task history lives.
- **Your `work/<id>` code worktree** lives in the delivery plugin's territory,
  `$XDG_STATE_HOME/balls/plugins/<delivery>/<project-path>/<id>/` — the project
  path **mirrored** (not percent-encoded). `bl claim` prints it; `bl show <id>`
  and `git worktree list` read it back. It is computed, never stored.

## Invariants — do not violate these

These hold across every command. Break one and your work silently fails to land
or you collide with another agent.

- **Prime first, every session:** `bl prime --as ID`. It founds the substrate on
  first run and syncs after; nothing else readies the checkout.
- **Always pass `--as ID`.** Every claim/close/prime is stamped with a worker
  identity. Do not let an LLM invent its own name — models collapse to the same
  few names and step on each other's claims. Have the harness pick one at session
  start and pass it through.
- **All edits go in the claimed `work/<id>` worktree, never on `main`.** `bl
  close` squashes the *worktree's* diff; a stray edit on `main` is invisible to
  it — the task closes clean while leaving your change behind, undelivered.
- **Status is derived, never stored.** A task has no `status` field; `ready` /
  `blocked` / `claimed` are computed on read (see `bl list --skill`). A closed
  task has no file — absence is the record.
- **Close is gated by the repo's own `pre-commit` hook.** Delivery folds `main`
  in first, then runs the hook; a failure aborts the close and leaves the task
  claimed for the fix.
- **stdout carries one product; parse with `--json`.** `create` prints the new
  id, `claim` prints the worktree path — nothing else. Every other mutating verb
  is silent on stdout; confirmations and the op log go to stderr. For `list` /
  `show`, always parse `--json` (the lossless bedrock record), never the tty
  view.

## Commands

Run `bl help` for the terse one-line directory. Full usage for any command is
`bl <command> --skill` (`--help` is an alias). Grouped by lifecycle:

- **Deliverable** — author a `tasks/<id>.md` change:
  `create` · `claim` · `unclaim` · `update` · `close` · `import`
- **Reads** — project the store, author nothing:
  `show` · `list`
- **Checkout lifecycle** — act on this checkout, not a ball:
  `prime` · `sync` · `install` · `conf`

## How the pieces relate

- **Tasks gate each other** through blocker edges (`--needs` / `--blocks` /
  `--subtask-of`). The full dependency model — and why filing flat balls is a
  parallelism decision — is in `bl create --skill`.
- **Behavior beyond "commit the task file" is plugins**, subprocesses wired in
  the landing's `[hooks]` schedule (`bl-tracker` syncs the remote, `bl-delivery`
  owns the worktree). The schedule and its ordering rules are in `bl conf
  --skill`; moving a plugin's binary or config between branches is `bl install
  --skill`. Binaries never travel — a schedule naming a binary you lack refuses
  cleanly, showing the owner's `[source]` acquisition hint when one is authored
  (you run it; balls never does).
- **The store remote resolves the same way on every command** — `--remote` >
  the per-checkout binding/stealth sentinel > `origin`. The ladder is spelled
  out in `bl prime --skill`.

## Operating against a bare project repo

A common deployment is a **bare** project repo (no working tree at the root). A
`git status` at the bare root is fatal by design (`must be run in a work tree`),
not a broken repo — use `bl list` for task state and run `git status` / `git
diff` inside your `work/<id>` worktree. All `bl` verbs run from the bare root.

In a **non-bare** repo the verbs work the same, with one trap: delivery moves
`main` by plumbing and never touches the root checkout's working tree, so after
a close the root is **stale** — refresh it (`git checkout` / `git reset --hard`)
before touching it. `bl` never resets it for you; it may hold uncommitted work.
