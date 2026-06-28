# balls

**balls** — the **B**ranching **A**gent **L**abor and **L**ogistics **S**ystem — is a git-native task tracker for the point where you stop running one agent and start running a fleet.

That is where ordinary trackers come apart. Two agents claim the same task and overwrite each other's work. A third stops one commit short of done and reports success. `main`'s history turns into a slurry of bookkeeping commits braided through real code. And the tracker that was supposed to coordinate all of this wants a daemon, a second database, or a network round-trip on every read — so the whole fleet stalls the moment one of them is unreachable.

balls is built so none of those failures have anywhere to take root. Tasks are markdown files (TOML frontmatter) committed to dedicated git branches, and **nothing touches `main` — that property is structural, not a convention**: base balls never opens your project repo, so it *cannot* leave a commit there. Every claim hands the agent its own git worktree, so parallel work is isolated by construction, not by etiquette. Status is *derived, never stored* — claimed, blocked, and ready are computed from a single occupancy field, so two agents can't hold conflicting views of who owns what. And every operation is atomic — a claim, a close, a sync either lands whole or leaves nothing to repair. An interrupted op's *entire* recovery protocol is to run it again: it detects whatever already landed and converges onto it, so a crashed agent never wedges the store or strands a half-merge for anyone to untangle. Because the whole system rides one VCS — fetched by `git fetch`, synced by `git push`, its history read by `git log` — there is **no database, no daemon, no external service** to run, nothing that can be down when the work needs it. balls works offline, and it degrades gracefully: strip every plugin and it is a pure local task list any collaborator can read, diff, and hand-edit with stock git.

The CLI is `bl`. It runs the full spectrum — from one operator keeping a standalone backlog (no fleet, no codebase; the whole thing is task files you could keep by hand with `vi` and `git`) through a single developer driving a dozen agents, up to an entire team running enterprise workflows and external integrations across many machines — on the same two branches the whole way up.

> **Companion documents go deeper.** `SKILL.md` (`bl skill`) is the operational guide for an agent driving `bl`. `docs/architecture.md` is the frozen design reference (§0–§16) — the authority for every claim in this README. `docs/release-notes-greenfield.md` narrates the greenfield (0.x) model and what changed from legacy; `docs/demonstration.md` is a captured end-to-end proof run against the shipped binary. This file is the introduction.

### Default workflow

One agent takes a task all the way through: `bl claim → work → bl close → done`. There is **no `review` step and no separate reviewer**: claiming gives you a code worktree, and `bl close` delivers it (squashes your work to `main`, then pushes `main` to the code remote — fail-soft) and tears the worktree down in one move. Balls does not assume a separate reviewer; if you want a split submit/approve flow, add a review gate as an ordinary close-blocker subtask (or a forge plugin that mints one at claim) — submission itself is git-native work, never a close phase. Otherwise the agent that claims also closes — which keeps agents from stopping short of finishing, the single most expensive failure mode in an agent-driven workflow.

---

## Installation

Balls ships as a small Rust binary `bl` plus three sibling plugin binaries (`bl-tracker`, `bl-delivery`, and the opt-in `bl-chore`). The only runtime dependency is `git`.

### From source (recommended)

```bash
git clone https://github.com/mudbungie/balls.git
cd balls
make install
make hooks     # one-time per clone: install the repo-local pre-commit hook
```

`make install` builds release binaries and installs four executables to `~/.local/bin/`: `bl` (core, plus a `balls` alias symlink), `bl-tracker`, `bl-delivery`, and `bl-chore`. Wiring is by name: the hook schedule (`config/plugins.toml`) lists plugin names, and `bl prime`/`bl install` bind each name to the binary of that name installed **beside `bl`** — a local, gitignored `config/plugins/bin/<name>` symlink that dispatch then resolves (§6). The seed wires `bl-tracker` and `bl-delivery`; `bl-chore` installs beside them but stays dormant until you schedule it (opt-in — see Plugins). A core-only install leaves `bl prime` founding a stealth, plugin-less task list: remotes and code worktrees silently never engage. Install the scheduled plugins beside `bl` and they wire themselves. Make sure `~/.local/bin` is on your `PATH`.

`make hooks` wires the repo-local pre-commit hook (clippy, 300-line cap, tests, 100% coverage). Run it once per clone; it is not part of `make install` because a user installing the binary should not have hooks attached to whatever repo they happen to be in. The coverage check requires `cargo install cargo-tarpaulin`.

To remove everything `make install` placed:

```bash
make uninstall
```

### From crates.io

```bash
cargo install balls
```

`cargo install` places `bl` in `~/.cargo/bin/`. To get the plugins beside it, build and copy `bl-tracker`, `bl-delivery`, and `bl-chore` next to `bl` (or use the source install above).

### Cross-compilation

```bash
cargo install cargo-zigbuild
cargo zigbuild --release --target x86_64-unknown-linux-gnu
cargo zigbuild --release --target aarch64-unknown-linux-gnu
cargo zigbuild --release --target x86_64-apple-darwin
cargo zigbuild --release --target aarch64-apple-darwin
```

### Verify

```bash
bl --version
cd your-repo
bl prime --as you          # founds the substrate on first run
bl create "My first task"
bl list
```

Balls is MIT licensed. See `LICENSE`.

---

## The model in brief

### Two branches, never `main`

State lives on **two branches** of your repo, each with one job and its own transport discipline:

- **`balls/config`** — the **landing**. Holds `config/` (this checkout's config: which plugins run, where the store syncs from). Path-derived, single-owner, never pushed by balls.
- **`balls/tasks`** — the **store** (default name; `config.tasks_branch` names it). Holds `tasks/<id>.md`, one file per task. Shared, sync-merged.

`config/` and `tasks/` are top-level folders on *both* branches always, so the code reads config from the landing ref and tasks from the store ref with no special-casing of what else a branch carries. `tasks_branch` can never *name* the landing, though: the two checkouts are worktrees of one repo, and git refuses a branch checked out twice, so the coincident name is refused outright. "Nothing on `main`" is structural: base balls never opens the project repo, so it *cannot* leave a commit there. The split exists because config and tasks have opposite transport disciplines — config is install-replaced (destructive, no merge), the store is sync-merged (union, fast-forward-only) — and one ref cannot carry both safely.

### State lives outside the repo (XDG)

balls does not keep its checkouts in your project tree. Per invocation path, the landing and store live under `$XDG_STATE_HOME/balls/clones/<percent-encoded-path>/` as `config/` and `tasks/`. Code worktrees live in the delivery plugin's territory at `$XDG_STATE_HOME/balls/plugins/<delivery>/<project-path>/<id>/` — the project path is **mirrored** there, not percent-encoded, so the build dir carries no `%` (which would break `cargo`/`rust-lld` linking; the clones/tracker dirs hold only git data, so they keep percent-encoding). You rarely touch these directly — the verbs read and write them — but that is where `git log`/`git show` of task history lives.

### Status is derived, never stored

A task has no `status` field. The three live states are computed on read:

- **claimed** — the `claimant` field is set (someone holds it).
- **blocked** — unclaimed, but an unresolved `claim`-blocker remains.
- **ready** — unclaimed with every `claim`-blocker resolved; claimable now.

A closed task has **no file** — absence is the resolution. Its history (including the delivery commit on `main` tagged `[bl-xxxx]`) is the record. `bl show <id>` and `bl list -s closed/--all` reconstruct dead tasks from `balls/tasks` history, walking newest→oldest.

### `--json` is bedrock

The human-facing output of `list`/`show` paints derived columns — the status ladder, the tree, ISO-8601 dates — none of them stored. `--json` is the orthogonal **bedrock** projection: raw stored frontmatter only, literal integer timestamps, no derived field. It is the round-trippable "what's actually there" and the supported machine contract. Parse `--json`, never the human render.

---

## Commands

| Command | What it does |
|---------|-------------|
| `bl prime [--as ID] [--remote URL] [--center URL] [--install URL]` | Ready this checkout: **founds the substrate on first run** (no separate `init`), then syncs. Re-prints the worktree path of every task you still hold. Run at session start. `--remote`/`--center` both name the store remote (`--remote` wins if both are given). |
| `bl sync [BRANCH] [--as ID]` | Pull the store from the remote (fetch + fast-forward). No arg syncs the configured store branch. |
| `bl list [-s\|--status ready\|blocked\|claimed\|closed] [--all] [--tag T] [--json]` | List tasks. Default = live (non-closed). `-s closed` (or `--all` for live+dead) reconstructs archived tasks from history. |
| `bl show <id> [--json]` | Task detail. A closed id still resolves (reconstructed from history). |
| `bl create "TITLE" [--body B] [-p N] [-t TAG] [--parent ID] [--needs ID[:OP]] [--blocks OP\|ID:OP] [-m MSG] [--as ID]` | File a task (`--body` sets the markdown body, `-m` the commit note). Prints the new id. |
| `bl import [--as ID]` | Bulk-create tasks from `--json` bedrock records on stdin — the inverse of `show --json`. Ids and timestamps are ingested verbatim (no minting, stamping, or gating); an existing id is refused (use `update` to modify). For migration, restore, and federation joins (§16). |
| `bl claim <id> [--as ID]` | Start work: materialize the `work/<id>` worktree, take occupancy. **Prints the worktree path** to stdout. |
| `bl unclaim <id> [--as ID]` | Release a claim, remove the worktree. |
| `bl update <id> [--title T] [--body B] [--parent ID\|--no-parent] [-p N\|--no-priority] [-t TAG] [--no-tag TAG] [--needs ID[:OP]] [--no-needs ID] [key=value] [-m MSG]` | Overwrite **any** field: `--title`/`--body`; set or clear the `--parent`/`-p` scalar; add (`-t`) or drop (`--no-tag`) a tag; set (`key=value`) or remove (`key=`) a preserved extra; add (`--needs`) or unlink (`--no-needs`) one of this task's own blockers. Only reciprocal `--blocks` (an edge on ANOTHER task) stays **create-only**. `-m` is the commit note. |
| `bl close <id> [-m MSG] [--as ID]` | Deliver (squash `work/<id>` → `main`) + archive the task + tear down the worktree + push `main` to the code remote (`origin`, fail-soft — a no-op without an `origin`). |
| `bl install [PATH] [--from REF] [--to REF] [--bin NAME=PATH] [--as ID]` | Copy a committed path between branches (adopt/publish plugin config). `PATH` defaults to `config/`, `--from` to the configured upstream, `--to` to the landing; `--bin NAME=PATH` names a referenced plugin's local binary explicitly (else beside `bl`, then `$PATH`). A folder source mirrors (deletions propagate), a file/glob source unions. |
| `bl conf [KEY]` | Read or write this checkout's **local** config (never synced), with provenance. No arg dumps every resolved value with its layer and source file; one `KEY` reads that value. Write with `bl conf <set\|append\|prepend\|remove> KEY VALUE…`; `KEY` ∈ `task-remote`, `task-branch`, `log-level`, `<op>.<pre\|post>`, `show`, `list`. |
| `bl skill` | Print the agent guide (`SKILL.md`) — the full manual. |
| `bl help` | Print the terse command directory (also `--help`/`-h`). |

There is **no `init`** (folded into `prime`), **no `review`** (folded into `close`), **no `ready`** (it is `bl list --status ready`), and no `remaster`/`resolve`/`reopen`. Subtraction is the design discipline: a new verb is a smell.

### Session start

Run `bl prime` at the start of every session:

```bash
bl prime --as YOUR_IDENTITY
```

`prime` is idempotent. On first run it **founds** the local substrate — seeding `config/` from the install defaults and creating the store — then syncs with the remote. Re-running converges to a no-op. To point a fresh checkout at a shared project, pass the remote once: `bl prime --as ID --remote <git-url>` (`--center <git-url>` is the same store-remote knob in federation framing; `--remote` wins if both are given). Pass `--install <git-url>` to also adopt that center's `config/` — a **single hop**, not a walk: a center's config names its own store branch, never another config to chase.

### Identity

Every claim/close/prime is stamped with a worker identity, resolved from `--as ID`, else `$USER`, else the literal `"unknown"`. **Don't let an LLM invent its own name** — language models are not RNGs and collapse to the same handful of names across sessions (you end up with three Junipers stepping on each other's claims). Source the randomness outside the model: have the agent harness pick a name at session start and pass it via `--as`. A portable recipe is `shuf -n1 /usr/share/dict/words`. In Claude Code, a `SessionStart` hook in `~/.claude/settings.json` that exports the name for the agent to pass as `--as` works well.

---

## Task schema

`tasks/<id>.md` is TOML frontmatter (fenced by `+++`) plus a free-form markdown body:

```markdown
+++
title = "Refactor the foo system"
created = 1748357520           # unix seconds; storage is always unix-time, display renders ISO-8601
updated = 1748443920
claimant = "you@example.com"   # occupancy: present ⇒ claimed, absent ⇒ unclaimed. NO status field
parent = "bl-1000"             # containment only — builds the tree, gates nothing
priority = 2                   # optional; lower = higher priority; absent sorts last
tags = ["refactor", "infra"]

[[blockers]]                   # the one relational primitive
id = "bl-1100"
on = "claim"                   # can't be CLAIMED until bl-1100 resolves (a dependency)

[[blockers]]
id = "bl-1200"
on = "close"                   # can't be CLOSED until bl-1200 resolves (a gate)
+++

Free-form markdown body.
```

- **The id is the path.** `<id>` is the filename basename — the sole source of truth, no `id:` field, no index. `git log -- tasks/<id>.md` works by id directly.
- **TOML everywhere.** Frontmatter and config are both TOML (one pure-Rust serializer, no C dependency). It exports losslessly to JSON for tooling.
- **Unknown keys are preserved** on writeback — the opt-in seam for a team's own field (e.g. a `state:` pipeline column read by their own display plugin), never a core field.

### Blockers and the dependency model

The one relational primitive is a blocker edge `{id, on}` on the *blocked* task: "this task can't do op `on` until task `id` resolves." `on` is **any** op, but two have create-time sugar:

- `--needs B[:OP]` — add a blocker on this task (default `OP = claim`, a dependency: can't be claimed until B closes).
- `--blocks OP` / `--blocks ID:OP` — the reciprocal: gate another task's op on this one. `--parent X --blocks close` makes X a parent that can't close until this child does (a gate).

`--parent` is **containment only** — it builds the display tree and gates nothing. An "epic" is just a task with children; to make a parent wait on its children, add explicit `--needs`/`--blocks` edges. Core enforces blockers: a `claim` of a blocked task or a `close` with an open gate is refused, naming the blocker. There is no special gate type — "review before close," sign-offs, and build gates are all emergent (a gate child plus a tag), never core rules.

---

## Plugins

Behavior beyond the base (commit config to the landing, task files to the store) is **plugins** — single binaries, dispatched as subprocesses with no in-process or privileged path. The schedule is config: `config/plugins.toml` on the landing has a `[hooks]` table mapping `<op>.<phase>` to an ordered list of plugin names (list position = run order). Each name resolves through a local, gitignored `config/plugins/bin/<name>` symlink that `bl prime`/`bl install` bind to the binary installed beside `bl`; a scheduled name whose binary is missing at founding is pruned from the seed, so a remote-less or plugin-less box still works.

The shipped seed (`default-config/plugins.toml`):

```toml
[hooks]
"sync.pre"     = ["bl-tracker"]                  # import remote state first
"prime.pre"    = ["bl-tracker"]
"install.pre"  = ["bl-tracker"]                  # fetch the center's config to adopt (§13 prime --install)
"prime.post"   = ["bl-delivery", "bl-tracker"]   # re-materialize still-claimed worktrees + print their paths, then settle store content (fetch-ff + push)
"claim.post"   = ["bl-delivery", "bl-tracker"]   # worktree (prints its path), then the push (tracker last)
"unclaim.post" = ["bl-delivery", "bl-tracker"]
"show"         = ["bl-delivery"]              # read-op (single phase): fold the worktree path into the human render
"close.pre"    = ["bl-delivery"]              # deliver (squash) before the seal
"close.post"   = ["bl-delivery", "bl-tracker"]   # teardown + push code main to origin (fail-soft), then push the store
"create.post"  = ["bl-tracker"]
"update.post"  = ["bl-tracker"]
"import.post"  = ["bl-tracker"]                  # imported records sync like any mutate (§16)
```

Two plugins ship by default and are wired by the seed config:

- **bl-tracker** — the only component that talks to a remote: fetch + fast-forward on sync, push after each op, found/adopt on prime. Strip it (or configure no remote) and the store stays local-only — "stealth" is not a mode, just a `tasks_branch` with no remote behind it.
- **bl-delivery** — owns the `work/<id>` code worktree: materialize on claim, squash-deliver + tear down on close, then push `main` to the code remote (`origin`, fail-soft — symmetric with the tracker's store push; bl-2656). It is **kind-blind** (never branches on task type) and stateless across ops (the worktree path is a pure function of the binding and id). Base balls never opens the project repo, so "nothing in the project tree" is structural — only this plugin touches your code.

A third, **bl-chore**, ships but is **opt-in** (not in the seed schedule): wire it with `bl conf prepend claim.post bl-chore` and it mints one tagged close-gate child per configured chore at claim, so the claiming agent must discharge them before `bl close` — a forcing-function checklist, not enforcement.

A plugin contributes by **editing the change worktree** (the task file), never by a parsed return value — core parses nothing back (there is no return channel, §7). Its stdout is the **user-facing channel**, forwarded verbatim to the invoker's stdout: "`bl claim` prints the worktree path" is bl-delivery printing there, and `bl prime` re-prints the path of every task you still hold the same way. Its stderr is enveloped line-by-line into the per-clone op log. A non-zero exit aborts the op and rolls prior plugins back in reverse. That is the whole protocol. See `docs/architecture.md` §6–§8 for the full contract.

### Delivery: one path; forge changes who merges

There is one delivery path: `close.pre` squashes `work/<id>` into the integration branch as one commit whose subject carries the `[bl-id]` delivery tag — and skips the squash when that incarnation already delivered (a `[bl-id]` commit on integration since the branch forked), whoever performed the merge.

- **Forge** (opt-in, ships separately per-forge) is not a delivery variant — it never hooks `close.pre`. The forge plugin mints an **approval gate child** at `claim` (an ordinary close-blocker, not a special mechanism) and closes it at `sync` when the PR merges, unblocking the next close. Submission is git-native work: push `work/<id>` and open the PR yourself, with the `[bl-id]` tag in the PR title — that tag is what lets the post-merge close recognize the squash-merge as the delivery and skip the local squash. "Forge review" is not a mode — it is the ordinary close plus a gate child, enforced by core's close-blocker guard.

Don't confuse a forge plugin with an **issue-tracker plugin** (Jira, Linear, GitHub Issues): same plugin protocol, unrelated job. Issue-tracker plugins mirror backlog state; forge plugins drive the merge gate. Both ship separately, not bundled here.

---

## Bare-repo deployment

A common deployment is a **bare** project repo (no working tree at the root) — the worktree/merge model makes a direct commit to the working branch a git-level impossibility rather than a discouraged convention. Then:

- `git status` at the bare root is fatal *by design* (`must be run in a work tree`), not a broken repo. For task state use `bl list`; for code state run `git status`/`git diff` inside your `work/<id>` worktree.
- All `bl` verbs run from the bare root.

Because state lives in XDG and the two branches are path-derived per checkout, multiple clones of one project share a store by naming the same `tasks_branch` remote — federation is many landings pointing at one store branch. The landing is never shared (it has no merge); only the store is.

---

## Removing or abandoning tasks

- A task you decided against (dupe, stale): `bl unclaim <id>` if you hold it, then `bl close <id>` — an empty deliverable archives without delivering code.
- `update` overwrites **every** ball field — no create-only split. It sets (`--title`/`--body`/`--parent`/`-p`/`-t`/`key=value`/`--needs`) and clears (`--no-parent`/`--no-priority` blank a scalar, `--no-tag`/`--no-needs` drop a member, a bare `key=` removes an extra) — so a mis-wired or cyclic blocker, a stale parent, or a wrong title is fixed in-band, never with store-file surgery. The lone create-only flag is reciprocal `--blocks` (an edge naming this task on ANOTHER task), since that is not this task's own field.

---

## Releasing

Releases to [crates.io](https://crates.io/crates/balls) are automated via [release-plz](https://release-plz.dev/) and GitHub Actions. The normal flow:

1. Land feature work on `main` with the project's commit style — a short title with a `[bl-xxxx]` trailer, optionally followed by a body. `bl close` does this for you: it squashes the delivery onto `main` and **auto-pushes `main` to `origin`** (fail-soft), so delivered work reaches the remote with no manual `git push`. Every non-`balls:` commit is picked up by release-plz's changelog.
2. On every push to `main`, `.github/workflows/release-plz.yml` opens (or updates) a **Release PR** that bumps `Cargo.toml`, regenerates `CHANGELOG.md`, and lists the commits going into the release.
3. Review the Release PR. CI (`.github/workflows/ci.yml`) runs `cargo test`, `cargo clippy`, line-length + 100% coverage checks, and `cargo publish --dry-run`.
4. Merge the Release PR. release-plz tags `vX.Y.Z`, creates a GitHub release, and publishes to crates.io.

### Bump signaling

Source commits here aren't Conventional Commits, so release-plz can't infer minor/major from `feat:`/`fix:` prefixes. Instead, `release-plz.toml` configures bracketed markers that compose with the `[bl-xxxx]` style:

| Marker    | Bump  | When to use |
|-----------|-------|-------------|
| `[major]` | major | Breaking change — task file format, CLI flag removed, etc. |
| `[minor]` | minor | New user-visible capability — new command, config option, behavior. |
| *(none)*  | patch | Default. Bugfix, refactor, doc, internal cleanup. |

Put the marker anywhere in a commit message that lands in the release window — release-plz matches the regex against the full commit text. The marker must be a standalone bracketed token (so prose mentions like `[minor]`/`[major]` describing the convention, wrapped in backticks, don't self-trigger a bump). If multiple commits in the window are marked, the highest bump wins.

### Manual release (fallback)

```bash
# on main, with a clean tree
cargo test && cargo publish --dry-run
# bump version in Cargo.toml, update CHANGELOG.md
git commit -am "Release vX.Y.Z"
git tag vX.Y.Z
git push origin main --tags
cargo publish
```

`CHANGELOG.md` is release-plz-owned — don't hand-curate `[Unreleased]`; write rich commit bodies instead.

---

## Library usage

balls is published as the `balls` crate and its modules are public, but the supported, stable surface is the **`bl` CLI** — in particular `--json` on the read verbs, the bedrock projection. The crate API tracks the internal greenfield architecture (`task`, `lifecycle`, `checkout`, `plugin`, …, documented in `src/lib.rs`) and may change between releases. For programmatic integration, prefer shelling out to `bl ... --json`: agents already have shell access, so `bl list --status ready --json` is a tool call with no adapter to maintain.

---

## Why not existing alternatives

### Beads

Beads was right about the core insight: agents need structured, queryable, persistent task state — not markdown files strewn across a repo. Balls is built on the same realization. Both keep task state out of `main`'s commit history so feature work and bookkeeping don't interleave; balls does this with dedicated git branches, beads with a separate database. The question we answer differently is what holds that state.

Beads uses Dolt — a version-controlled SQL database. That buys cell-level merging and sub-millisecond queries, both genuinely nice on large task sets. The cost is running two version-control systems side by side: git for code, Dolt for tasks — two histories to keep consistent, two merge models, two remotes, and a database binary every collaborator installs.

Balls asks whether one VCS can do both jobs. The two-branch design keeps task data fully out of `main`'s commit graph — same separation — but stores it in the same git repository, fetched by the same `git fetch`, pushed by the same `git push`. A collaborator who clones the repo gets the backlog; one without `bl` installed can still read, diff, and hand-edit task files with stock git. There is no second system to operate. The tradeoff is real: Dolt's cell-level merge is strictly more granular than git's file-level merge. Balls mitigates with one file per task (conflicts are per-task) and a text-mergeable TOML schema, but doesn't match per-cell precision. The bet is that one VCS beats two whenever one is sufficient — and for a backlog of tasks, git is sufficient.

### Cline Kanban

Cline Kanban provides a visual board for agent orchestration with worktree-per-task isolation. It solves the human attention problem well. But it's local-only with no multi-machine story, closed-source infrastructure, and tightly coupled to the Cline ecosystem despite claiming agent-agnosticism. There is no durable shared state — each developer's board is independent.

### GitHub Issues / Jira / Linear

Traditional trackers weren't designed for agent workflows. They require network round-trips for every read, can't be queried offline, don't support the claim-and-worktree lifecycle, and have no concept of local-first operation. They remain the right tools for human project management; balls integrates with them via issue-tracker plugins rather than replacing them.

### The balls approach

Balls takes the core insight — structured task files, dependency tracking, agent-native CLI — and implements it on the only infrastructure every developer already has: git. Tasks are files. Sync is push/pull. History is git log. Collaboration is merge. There is nothing to install except a small CLI and its sibling plugins, nothing to configure that isn't a committed TOML file, and nothing to operate except git.
