# balls — greenfield (0.x) release notes

The greenfield rewrite is a **breaking redesign** of how balls stores state and
runs operations. It replaces the legacy (≤0.4.x) model wholesale. This note
explains what changed, why, and how to move across. The numbered design spec is
[`docs/architecture.md`](architecture.md) (§0–§16); a captured end-to-end proof
is [`docs/demonstration.md`](demonstration.md).

> `CHANGELOG.md` (release-plz-owned) carries the per-version commit log. These
> notes are the narrative companion: the model shift, not the diff.

---

## TL;DR

- **Two branches, folder-namespaced.** State left `main` entirely. Config rides
  `balls/config` (the *landing*, holding `config/`); tasks ride a store branch
  (default `balls/tasks`, holding `tasks/<id>.md`). `main` is now purely your
  code — `git log --oneline main` is a clean, `[bl-xxxx]`-tagged changelog.
- **State lives outside the repo (XDG).** Checkouts moved out of `.balls/` in the
  repo to `$XDG_STATE_HOME/balls/clones/<encoded-invocation-path>/`. The repo no
  longer carries working balls state.
- **TOML + markdown, not JSON.** A task is `+++` TOML frontmatter plus a free-form
  markdown body. Timestamps are integer unix seconds (rendered ISO-8601 for
  humans). Config is `config/{balls,plugins}.toml`.
- **No `status` field.** ready / blocked / claimed / closed are *derived* on read.
- **`claim → work → close`.** `bl review` is gone; close delivers and tears down
  in one move. No separate reviewer by default.
- **Plugins wired in `config/plugins.toml` `[hooks]`.** One ordered name-list per
  `<op>.<phase>`. The old filesystem symlink registry is retired.
- **Verbs removed:** `init`, `review`, `ready`, `remaster`, `doctor`, `reopen`,
  `repair`, `link`, `resolve`. Most folded into a smaller, composable set.

---

## The model, in full

### Two-branch substrate (§2, §12)

One repo, two state branches. The **landing** (`balls/config`) is the sole
authority for capability policy: everything in `config/` is the install-default
the box runs by — config *is* the remote-code-execution surface (which plugins
run, in what order). The **store** (`balls/tasks` by default; the landing names
it via `tasks_branch`) holds the backlog. Many checkouts of one project share one
store branch on the remote while each keeps its own landing — that is
*federation* (§12), keyed by invocation path under XDG.

### XDG, not in-repo (§1)

Working checkouts live under
`$XDG_STATE_HOME/balls/clones/<percent-encoded-path>/` as `config/` (landing) and
`tasks/` (store). Code worktrees live beside them under the delivery plugin's
scratch area. The verbs read and write these for you; that is also where
`git log`/`git show` of task history lives.

### Implicit founding — no `init` (§12)

There is **no `bl init`**. `bl prime` founds the substrate on its first run
(seeding a fresh landing from the install-default `default-config/` folder) and
then syncs; re-running converges to a no-op. With no remote, founding is
**local-only / "stealth"** and just works. Point a checkout at a shared project
once with `bl prime --remote <url>` (`--center <url>` names the same store
remote in federation framing; `--remote` wins if both are given), or
`--install <url>` to also adopt that center's `config/` — `prime --install`
fuses prime + install + prime in one call. It is a **single hop**, not a walk:
a center's config names its own store branch, never another config to chase.

### `install` is a path-copy where *shape decides* (§6)

`bl install <path>` copies a committed path between branches. The shape of the
source decides the merge:

- a **folder** source **mirrors** (the destination is made to match — additions
  *and deletions*), so re-installing a center's `config/` wipes local divergence;
- a **file or glob** source **unions** (copy-in, no deletions).

This folder-mirror vs glob-union split is counterintuitive but load-bearing — it
is the one line of the model worth memorizing, because it is the difference
between "adopt the center wholesale" and "graft one capability in."

### Plugins: `[hooks]`, sibling binaries, pruned if absent (§6)

`config/plugins.toml` has a single `[hooks]` table: each `<op>.<phase>` →
ordered list of plugin names, position = run order. The legacy registry is
retired; a name now resolves through a **local, gitignored**
`config/plugins/bin/<name>` symlink that `bl prime`/`bl install` bind to the
binary installed beside `bl`. A scheduled name whose binary is missing at
founding is pruned from the seed, so a remote-less or plugin-less box never
aborts. Two ship by default and are seeded:

- **tracker** — the only remote-talker: fetch + ff on sync, push after each op,
  found/adopt on prime. (The tracker — never core — does the remote fetch on
  `prime --install`, via an `install.pre` hook.)
- **bl-delivery** — owns the `work/<id>` code worktree end to end.

To ship org defaults, replace the seed folder
`$XDG_CONFIG_HOME/balls/default-config/` (`balls.toml` + `plugins.toml`) — no core
edit (§1/§12). A non-default project branch triggers a tracker warning.

### Derived status, one blocker primitive (§3, §10)

No `status` field is stored. The 3-state ladder (ready / blocked / claimed) plus
closed (file absent from the store tip) is computed on read. The one relational
primitive is a blocker edge `{id, on}` on the blocked task. `--needs` /
`--blocks` / `--parent` set edges *at create* (`--parent` is containment only).
Core enforces them: a blocked `claim` or a gated `close` is refused, naming the
blocker.

### Lifecycle: claim → close (§8, §9, §11)

`bl claim` takes occupancy (the one `claimant` field) and materializes a
`work/<id>` worktree off `main`. `bl close` delivers (squashes the worktree onto
`main` as one `[bl-xxxx]` commit), seals the task-file deletion on the store, and
tears the worktree down — all in one op. `drop` is the same mechanics with intent
"abandon" (no delivery). There is no `review`; for a split submit/approve flow,
wire a `close.pre` approval gate explicitly.

---

## Migrating from legacy (§16)

Legacy stored tasks as JSON under `.balls/tasks/` and config in
`main:.balls/config.json`. The move is a **one-shot throwaway script**, not a
verb: `scripts/migrate-legacy.py`. It transforms *core* fields only
(`claimed_by→claimant`, `created_at→created`, `depends_on→blockers:[{id,on:claim}]`,
`description`+notes→body, …) and hands off to `bl prime` for everything ongoing.
Per-plugin legacy state (`external.<plugin>.*`) is dropped; each plugin's
greenfield port re-adopts its own territory. The rule is
**migrate-clean-or-delink**: transform only what maps deterministically — a
dangling parent is nulled, an unported plugin is left out of the hook schedule.
Full field map and runbook are in the script header and `architecture.md` §16.

---

## Compatibility & behavior notes

For anyone scripting against the binary, a few specifics that differ from older
docs and intuitions:

- **Core keeps stdout machine-clean; plugins own the user-facing prints.** Of
  core's own emissions, `create` is the only mutating verb that prints to
  stdout (the minted id, alone); the others print a terse confirmation to
  **stderr**, and the op log is on stderr too. A plugin's stdout, though, is
  forwarded verbatim to the invoker (§6) — that is where the worktree paths
  below come from.
- **`bl claim` prints the worktree path** (bl-delivery printing on
  `claim.post`); `bl prime` re-prints the path of every task you still hold,
  and `bl show <id>` folds a `worktree` line into the human render when the
  worktree exists on this machine. The path is **computed, never stored** —
  the bedrock `--json` projection carries only stored frontmatter, so it never
  appears there. `git worktree list` (the `work/<id>` line) is the git-side
  read.
- **`bl update` overwrites every ball field** — no create-only split. Retitle
  (`--title`), rewrite the markdown body (`--body`), set or clear the `--parent`
  and `-p` scalars (`--no-parent`/`--no-priority`), add or drop a tag
  (`-t`/`--no-tag`), set or remove a preserved extra (`key=value`/`key=`), and add
  or unlink one of the task's own blockers (`--needs`/`--no-needs`, the in-band
  fix for a mis-wired or cyclic edge). A blocker — or a stale parent or wrong
  title — really blocks, so it must be fixable without store-file surgery. The
  lone create-only flag is the reciprocal `--blocks` (an edge naming this task on
  another), since that is not this task's own field.
- **`--body` sets the ball's markdown body; `-m` is the commit note.** The commit
  subject is always the ball title (no override).
- **No `bl reopen`.** A closed task's history is the record; to revive one, revert
  the archive commit on the store branch (and, if a forge/issues plugin is wired,
  reopen the upstream issue first, or the plugin re-closes it).
- **`--json` is bedrock.** Raw stored frontmatter, integer timestamps, no derived
  fields — the supported machine contract. Parse that, not the human render.

---

## Status of this release

The greenfield core and the two shipped plugins (`tracker`, `bl-delivery`) are
live; the legacy→greenfield cutover has been executed and dogfooded end to end
(see `docs/demonstration.md`). Forge-gated delivery and the github-issues plugin
port are downstream follow-ups (separate, per-forge, not bundled here). The
standalone `bl install` verb is fully wired (`bl install [<path>] --from <ref>
[--to <ref>]`); `prime --install` remains the one-call adoption route for a
fresh checkout.
