# balls — Agent Skill Guide

You are using **balls** (`bl`), a git-native task tracker. Tasks are JSON files in the repo. Worktrees isolate your work. Git provides sync.

## The default flow: finish your own task

**One agent takes the task all the way through: claim → work → review → close → done.** That's the default. The `review` status is a checkpoint, not a stopping point. If nobody else is lined up to review your work, *you* do the close step yourself and the task is finished. Do not leave tasks sitting in `review` waiting for a reviewer who does not exist.

There is one inviolable rule: **never run `bl close` while you're standing inside the worktree it would delete.** `cd` back to the repo root first. The binary will refuse otherwise with `cannot close from within the worktree`.

Splitting the work across two agents — one submits, a different one approves — is an opt-in pattern, covered at the end of this guide. Don't reach for it unless the user has actually set up a reviewer. An agent that submits and then stops is an agent that didn't finish its job.

## The worktree is the unit of work

`bl` doesn't just track tasks — it tracks the code change a task delivers. Claim creates a worktree. Review squashes that worktree's branch back to main. Close tears the worktree down. The worktree isn't a convenience layer wrapped around your real workflow; while a task is claimed, it *is* your workflow.

So the rule reads as a consequence: while you hold a claim, edits go in the worktree, not on the operating branch. Editing main directly bypasses the lifecycle — the worktree never sees the change, `bl review`'s squash captures the wrong diff, and the delivery commit on main no longer reflects what the task shipped. The task can close perfectly cleanly while leaving drift behind it.

This binds the balls workflow, not your repo. Outside a claimed task, your tree is yours. But once `bl claim` has printed a path, that path is where the work goes.

## Operating against a bare hub

The recommended deployment is a **bare** central repo (`core.bare = true`): no work tree at the root, every change in a `.balls-worktrees/<id>/` checkout. The "repo root" the close rule names *is* that bare directory — `cd` there and `bl close` works. Three things that bite if you don't expect them:

- `git status` at the bare root is **fatal by design** (`fatal: this operation must be run in a work tree`), not a broken repo. To see state: `bl list` for tasks; `git status`/`git diff` *inside* your `.balls-worktrees/<id>/` worktree for code.
- Read-only and root commands (`bl prime`/`ready`/`list`/`show`, and `bl close`) all work from the bare root. `bl review` does its own squash via an internal detached worktree — nothing extra for you to do.
- The inviolable rule is unchanged and really means "not from inside the bl worktree." On a bare hub there is no checked-out main to stand in, so `cd <repo root>` before `bl close` means `cd` to the bare directory.

## Session Start

Run `bl prime` at the start of every session:

```
bl prime --as YOUR_IDENTITY --json
```

It syncs with the remote and returns:
- **claimed**: tasks you already own — resume in their worktrees.
- **ready**: open tasks available to claim, sorted by priority.

If you resume and find one of your own tasks sitting in `review`, that's not a wait state — it means you stopped short last time. Run `bl close` from the repo root and finish it.

### First-time setup in a repo

`bl init` bootstraps a repo that has never used balls. It is not a no-op:

- If the repo has zero commits, it creates an initial commit so balls has something to anchor to.
- It creates a `balls/tasks` orphan state branch (non-stealth mode) where task JSON lives, separate from `main`'s history.
- It adds `.balls/config.json`, `.balls/plugins/.gitkeep`, and a `.gitignore` entry for runtime state, then commits them to the current branch (`balls: initialize`).

If you're scripting against a fresh repo, expect `bl init` to add one commit to whatever branch you're on. Make any pre-existing commit you want on main *before* running it.

**No-git mode:** `bl init --tasks-dir /path` also works outside git repos. All commands work; `bl claim` requires `--no-worktree`; `bl review`/`bl close` are status flips with no merge.

## Commands

| Command | What it does |
|---------|-------------|
| `bl prime --as ID` [`--json`] | Sync, show ready + your claimed tasks. Run at session start. |
| `bl ready` [`--json`] [`--limit N`] | List open tasks ready to claim. `--limit N` caps output; text mode adds a `... and K more` footer when truncated. |
| `bl list` [`--status STATUS`] [`--closed`] [`--all`] | List non-closed tasks (`--status review` finds reviewables). `--closed` (alias `--status closed`) reconstructs archived tasks from the `balls/tasks` history; `--all` lists open and closed together. |
| `bl show TASK_ID` [`--json`] [`--verbose`] | Task details, including `delivered_in` sha after review. A closed task's id still resolves — `bl show` reconstructs it from the state-branch history. |
| `bl create "TITLE" [-d DESC] [-p 1..4] [-t TYPE] [--parent ID] [--dep ID] [--tag T]` | File a new task. Prints the new task id to stdout. See **Creating Tasks** below. |
| `bl claim TASK_ID` [`--no-worktree`] [`--sync`/`--no-sync`] | Start work: create worktree, set status=in_progress. `--no-worktree` skips worktree creation (required in no-git mode). `--sync` forces a remote round-trip on this claim (closes the offline-agent claim-race window); `--no-sync` skips one even if the repo's `require_remote_on_claim` is on. |
| `bl review TASK_ID -m "msg"` [`--sync`/`--no-sync`] | Squash to main, set status=review. Worktree stays. The task id is auto-appended to the subject — do **not** include `[bl-xxxx]` in your `-m`. `--sync`/`--no-sync` toggles a remote round-trip of the state-branch review commit (mirrors `bl claim --sync`); the repo can default to required via `require_remote_on_review`. In no-git mode, status flip only. |
| `bl close TASK_ID -m "msg"` [`--sync`/`--no-sync`] | Finish: archive task, remove worktree + branch. **Repo root only.** `--sync`/`--no-sync` toggles a remote round-trip of the state-branch close commit before the worktree is torn down; the repo can default to required via `require_remote_on_close`. A required-policy failure rolls back the local close and keeps the worktree for retry. In no-git mode, archives file directly. |
| `bl update TASK_ID status=in_progress --note "..."` | Multi-agent reject path: bounces a submitted task back to in_progress. |
| `bl update TASK_ID status=closed --note "..."` | Archive an unclaimed task (dupes, stale, decided-against). Archival not deletion — see **Removing unwanted or duplicate tasks**. |
| `bl update TASK_ID --note "text"` | Add a note. |
| `bl drop TASK_ID` | Release a claim, remove worktree. |
| `bl dep tree` [`--json`] | Show parent/child tree with deps and gates as inline annotations. |
| `bl remaster TARGET` [`--commit`] / `bl remaster --detach` | Re-point this repo's `balls/tasks` at the git remote `TARGET` (a shared task hub) and reconcile local-only tasks onto it. Per-clone by default; `--commit` writes the project-wide `.balls/config.json`. `--detach` severs shared history and goes standalone. Idempotent. See **Multi-repo: one project, many repos**. |
| `bl plugin enable NAME` [`--config-file PATH`] / `bl plugin disable NAME` / `bl plugin list` [`--json`] / `bl plugin policy NAME EVENT=KIND...` / `bl plugin show NAME` [`--json`] | Manage the effective plugins map. `policy` sets SPEC §11 per-event participant policy (`KIND` ∈ `required`/`best-effort`/`gating`); `--rm EVENT` drops one subscription, `--clear` removes the block (legacy `sync_on_change` fallback), `--no-legacy` writes an explicit empty map (the plugin participates in nothing). `show` prints one plugin's resolved per-event policy. Under `master_url` the writes land on the hub's `balls/tasks` (commit auto-staged; `bl sync` publishes); standalone repos get an in-place edit of the project's `.balls/config.json`. `list`/`show` report the source (`hub` vs `project`). `enable --sync-on-change` is deprecated — use `policy` to set per-event policy explicitly. |
| `bl doctor` | Read-only drift check. Reports the specific reason bl can't run here (or that it can but state has drifted) and names the command that fixes each. Never mutates — `repair` stays the only action verb. Run it when bl behaves unexpectedly or before trusting an unfamiliar repo. |

> **Note for agents:** the human-facing output of `bl list`, `bl ready`, `bl show`, and `bl dep tree` uses status glyphs and colors when stdout is a tty. Always prefer `--json` for parsing. If you must scrape human output, pass `--plain` (or set `NO_COLOR=1`) for stable, glyph-free, ASCII-only text — but the `--json` shape is the supported machine contract.

## Task Lifecycle

```
                          ┌─ local-squash (default): squashed to integration branch ─┐
open ──claim──> in_progress ──review──> review ──close──> archived
                     ^                    │      │
                     └────── reject ──────┘      └── blocked while open `gates` links exist
                          └─ deferred (opt-in): branch pushed, auto-gate child opened;
                             review still set, but close stays blocked until the forge
                             merges and the gate child closes ─┘
```

- **open**: available to claim.
- **in_progress**: claimed; whoever holds it owns it.
- **review**: work has been squashed to main; the task is one `bl close` away from done. In the default solo flow, this is a transient state — you flip through it as you finish. Only in a multi-agent setup does it mean "waiting on someone else."
- **deferred**: explicitly set aside with intent to revisit. Narrow meaning — see **Deferred vs closed** below. Not a trash can.
- **closed/archived**: task file archived from the state branch's HEAD (not main). The work itself, or the decision not to do the work, lives in main's git history.

If a reviewer does exist and rejects, they set status back to `in_progress`. You resume in your existing worktree; your next `bl review` re-merges main automatically.

A `bl close` is additionally blocked if the task has any open `gates` links — see the link-types table below.

### Removing unwanted or duplicate tasks

If a task shouldn't exist — a duplicate of another open ball, a stale idea from a past exploration, something you decided against — **close it. Don't defer it, don't leave it in the queue.** Closing archives the task file from the state branch's HEAD; it is *archival, not destruction* (the state-branch history still has it, so closed tasks are recoverable via git). There is no separate `bl delete` and you don't need one. To read a closed task back, `bl show <id>` resolves it directly (falling back to the history) and `bl list --closed` enumerates every archived task — both reconstruct from the state branch, so neither works in a stealth/no-git store.

For an unclaimed task, one command does the job:

```
bl update bl-xxxx status=closed --note "dup of bl-yyyy"
```

Duplicates also get a link so the relationship is discoverable in the archive:

```
bl link add bl-xxxx duplicates bl-yyyy
bl update bl-xxxx status=closed --note "dup of bl-yyyy"
```

For a task you already claimed and now regret: `bl drop` releases the claim, then `bl update ... status=closed` archives it.

### Deferred vs closed

Use `deferred` only for work you've decided not to do *now* with an intent to revisit later — waiting on an upstream, needs data you don't have yet, blocked on a decision outside the repo. If you aren't going to revisit it, close it. `deferred` is not a synonym for "I don't want to touch this" or "I'm not sure if this is real" — that's what `closed` is for. A growing `deferred` pile is a smell: those should be either promoted back to `open` or closed out.

## Workflow

The whole path, start to finish, done by one agent:

```
bl prime --as YOUR_ID
bl claim TASK_ID             # prints worktree path
cd <worktree path>           # all edits go here (see "The worktree is the unit of work")
# ... edit, commit ...
bl review TASK_ID -m "msg"   # squash to main, flip status to review
cd <repo root>               # required for close
bl close  TASK_ID -m "msg"   # archive task, remove worktree
```

`bl review` is the penultimate step, not the finish line. If you're not in a multi-agent setup with a configured reviewer, keep going and run `bl close` yourself.

Worktrees live at `.balls-worktrees/bl-xxxx`, on branch `work/TASK_ID`, with `.balls/local` symlinked for shared lock state.

Important:
- **Commit your work** before `bl review`. Review will `git add -A` anything left behind, but explicit commits give better history.
- **Edits live in the worktree** — see "The worktree is the unit of work" above for why this is load-bearing, not a convention.
- **Don't `bl update TASK_ID status=closed`** on a *claimed* task — it's rejected. Use `bl review` (which flips it to `review`), then `bl close` from the repo root. (For *unclaimed* tasks you want to remove — duplicates, stale ideas — `bl update status=closed` is the correct command; see **Removing unwanted or duplicate tasks** above.)
- **Don't hand-edit or rm files under `.balls/`** — that's the on-disk store, and direct edits corrupt the state branch. To release a claim use `bl drop`; to clean up orphans use `bl repair --fix`; to remove an unwanted task close it (see above). The restriction is about the store format, not the task lifecycle.

### What `bl review` does

1. Commits all uncommitted work in your worktree.
2. Merges main into your worktree (catches up; conflicts surface HERE, not on main).
3. Squash-merges your branch into main as a single feature commit.
4. Writes the delivery hint and flips status to `review` on the state branch.

If step 2 conflicts, review fails. Resolve in the worktree, then retry.

### Commit messages: 50/72 shape

`-m` is repeatable, exactly like `git commit -m … -m …`: the first `-m` is the commit title, each later `-m` becomes a body paragraph (blank line between). The delivery tag `[bl-xxxx]` is appended to the title automatically. No shell heredoc needed:

```
bl review bl-abcd \
  -m "Short imperative title under ~50 chars" \
  -m "Body paragraph explaining the change in detail. Wrap at ~72." \
  -m "Add more -m flags for more paragraphs."
```

A single `-m` value may still contain newlines (first line = title, rest = body), so `-m "$(cat <<'EOF' … EOF)"` keeps working. A single-line `-m "fix foo"` becomes `fix foo [bl-abcd]` with no body. Don't stuff a multi-sentence summary into one line; that produces an unreadable `git log --oneline`.

## Forge-gated review (opt-in)

Most repos use the default *local-squash* delivery: `bl review` squashes your branch to the integration branch immediately and you `bl close` yourself. Some repos opt into *deferred* delivery instead (`delivery.mode = "deferred"` in `.balls/config.json`) because their merges are produced by a forge — a GitHub PR, GitLab MR — after required review and CI. The full picture is README §Delivery Modes and `docs/SPEC-forge-gated-delivery.md`; here is only what changes for you as the working agent.

**What `bl review` does differently in deferred mode.** It does *not* squash to the integration branch. Instead it:

- pushes your `work/bl-xxxx` branch to `origin`,
- auto-creates a **gate child** task and links the parent to it (`parent gates child`),
- flips the parent to `review`, leaving `delivered_in` null.

It prints a recommended PR title ending in `[bl-xxxx]`, the gate child id, and the branch name. Open the PR (`gh pr create` or your forge's plugin) against the configured target branch, keeping that `[bl-xxxx]` in the title.

**The gate child is not yours to claim.** It is a marker that the parent is waiting on an external merge — exactly the "do not claim a `gates` target" rule from the Links section. Don't claim it, don't work it. It closes on its own when the forge merges the PR: either someone closes it by hand after merging, or a forge plugin's `sync` closes it automatically and carries the merge SHA back. Once it closes, the parent's `bl close` unblocks; run it from the repo root and `delivered_in` resolves from the `[bl-xxxx]` tag the forge merge put on the integration branch.

So in deferred mode your finish line moves: `bl review` is genuinely a handoff to the forge, not a transient flip-through. You're done once the PR is open and the gate child exists — *unless* you're also the one merging the PR, in which case carry it through to `bl close`.

**Backwards-compat caveat.** If you're on an old `bl` in a deferred-mode repo, `bl review` won't know to defer — it will local-squash and contaminate the integration branch with a premature commit. There is no guard against this; check `min_bl_version` in `.balls/config.json` and make sure your `bl --version` meets it before running `bl review` in an unfamiliar repo. (Old `bl close` is safe — it still respects the gate block.)

## Multi-agent: split submitter and reviewer (opt-in)

Only use this pattern if the user has actually asked for it or set up a separate reviewer agent. Otherwise the submitter should close their own task per the default flow above.

When the roles *are* split, the reviewer's job is to accept or send back work that's already squashed to main:

```
bl list --status review                       # find tasks awaiting review
bl show TASK_ID                               # status, claimant, delivered_in sha
git show <delivered_in sha>                   # inspect what landed on main
cd <repo root>                                # required for close — see hard rule above

# Approve:
bl close TASK_ID -m "approval note"

# Reject:
bl update TASK_ID status=in_progress --note "what to fix" --as reviewer
```

Important:
- **Don't edit files inside the submitter's worktree** to fix things — reject with notes and let them iterate.
- **Approval is durable.** Close archives the task file from the state branch HEAD and removes the submitter's worktree + `work/TASK_ID` branch. The squash commit on main carries the work.
- **Rejection keeps the worktree.** The submitter resumes in place; their next `bl review` re-merges main automatically.
- If accepted work needs to be undone, that's a `git revert` of the delivered sha on main, not a `bl close` thing — close still archives the task.

## Multi-repo: one project, many repos (opt-in)

By default a repo's task state branch (`balls/tasks`) is negotiated
against the same remote as its code (`origin`). A project spanning
several code repos can instead share **one** task store on a dedicated
hub. There are two ways to wire the hub link:

- **`master_url` (recommended, bl-ffb4 / bl-82a4)** — the hub's git
  URL lives in a small committed **pointer file**, `.balls/master.json`.
  Balls materializes its own clone at `.balls/state-repo/` (origin =
  that URL), and `.balls/config.json` + `.balls/plugins/` become
  **symlinks** into that clone — so the canonical config is literally
  the hub's. The project's own `.git/config` is untouched —
  `git remote -v` stays clean. A teammate's fresh `git clone` +
  `bl prime` is a complete onboard: the pointer bootstraps the
  state-repo and the symlinks resolve.
- **`state_remote` (legacy)** — point at the *name* of an existing
  project-side git remote. Works for the single-repo "shared hub via
  origin" case, but a fresh clone lands without that remote in its
  `.git/config` and stays *safe but unlinked* until a teammate runs
  `git remote add hub <url> && bl remaster hub`. Deprecated for the
  cross-repo case in 0.5.0 in favor of `master_url`. The link lives
  in `.balls/master.json` too (committed) or `.balls/local/config.json`
  (per-clone).

In both modes claim/review/close/sync are unchanged; only the orphan
ref retargets. The code remote is never disturbed.

- **Onboard with `master_url`:** in any clone — even one that has
  never run `bl init` — `bl remaster <hub-url> --commit` materializes
  `.balls/state-repo/`, writes the `.balls/master.json` pointer,
  swaps `.balls/config.json` and `.balls/plugins/` for symlinks into
  the hub, and auto-commits the result. Push it; everyone else's next
  `git pull && bl prime` joins automatically. Idempotent — re-running
  against the same hub is a no-op, and it also migrates a repo still
  on the pre-bl-82a4 in-config `master_url` shape.
- **Onboard with legacy `state_remote`:** `git remote add <name>
  <url>` then `bl remaster <name>`. `--commit` writes the *name* into
  shared config (which other clones still have to wire up to a real
  URL manually). Idempotent: re-running against the same hub is a
  no-op.
- **Unaware `bl init`:** a clone whose committed config names a hub
  it has no git remote for stays *safe but unlinked* — a usable
  isolated local store. `bl remaster` is the non-destructive way to
  fold those tasks into the project later. `bl init` never resets,
  force-pushes, or clobbers a shared branch.
- **Unreachable `master_url`:** first-time setup against a hub URL
  the clone can't reach (no access, VPN down, typo) **hard-fails**
  — `bl prime`/`bl init` surface the URL, the underlying fetch
  error, and three resolution paths (fix access, edit the pointer,
  detach). No silent fallback to a local orphan: a `master_url`
  client is a pure pointer, and drift between teammates is the
  exact failure mode the cross-repo model exists to prevent. Once a
  state-repo has materialized successfully, *later* offline runs
  soft-fail (work from the local cache, parity with normal git).
- **Leave:** `bl remaster --detach` forks the current branch into a
  fresh local orphan, restores `.balls/config.json` + `.balls/plugins/`
  as real files, and drops `.balls/master.json` — the repo is
  standalone again. Works **offline**: when the hub is unreachable
  and the state-repo never materialized, detach is the escape hatch,
  so it never requires network access.

### Config under `master_url`: one file, two materializations

The seam is filesystem-shaped (bl-82a4). `.balls/master.json` is a
tiny committed **pointer** — it carries only `master_url` (and the
legacy `state_remote`). The canonical config bl actually reads is
always `.balls/config.json`:

- **Standalone:** a regular committed file; `.balls/plugins/` a real
  directory.
- **Federated:** `.balls/config.json` is a symlink to
  `.balls/state-repo/.balls/config.json` (the hub's own config) and
  `.balls/plugins/` is a symlink to `.balls/state-repo/.balls/plugins/`
  (bl-1098). So the hub owns *all* policy — task knobs,
  target-branch, delivery, plugins — "master wins" outright, with no
  runtime layering.

This composes naturally with the bridge/proxy pattern below: config,
plugin secrets, sync schedules, and mirroring policy live in **one**
place across an N-clone federation, so client repos can't drift them.

- **Hub-aware tooling: `bl plugin enable|disable|list|policy|show`.**
  From any participant clone, `bl plugin enable <name> [--config-file
  PATH] [--sync-on-change]` writes the entry into the effective
  plugins map and creates an empty per-plugin config file if absent.
  `bl plugin disable <name>` removes the entry but keeps the
  per-plugin file (operators may want to preserve credentials).
  `bl plugin list [--json]` shows the effective map and its source
  (`hub` under `master_url`, `project` otherwise). `bl plugin policy
  <name> <event>=<kind> ...` edits SPEC §11 per-event participant
  policy and `bl plugin show <name>` inspects the resolved policy.
  Under `master_url` the writes land on `.balls/state-repo/`'s
  `balls/tasks` branch — run `bl sync` to publish. In standalone mode
  the writes update the project's `.balls/config.json` and
  `.balls/plugins/*` in place; commit them yourself.
- **Hand-editing is still supported.** Edit `.balls/config.json` and
  `.balls/plugins/<name>.json` through the project-root path (the
  symlinks transparently land you inside `.balls/state-repo/` in
  federated mode) or `cd .balls/state-repo` first. Commit on
  `balls/tasks` and push; every client reads it through the symlink
  immediately, and a fresh clone picks it up on the next `bl prime`.
- **Flipping to federated migrates, never refuses (bl-82a4).**
  `bl remaster <url> --commit` promotes the project's `plugins` map
  and any non-placeholder `.balls/plugins/*` files up into the hub
  (hub wins on a name clash — the project-side entry is dropped and
  named in the command's output), then makes `.balls/config.json` and
  `.balls/plugins/` gitignored symlinks. There is no silent-drift
  window and no manual move-it-yourself step.
- **Detach restores standalone shape.** `bl remaster --detach`
  replaces the symlinks with real `.balls/config.json` +
  `.balls/plugins/` carrying the hub's content at the moment of
  detach, and drops `.balls/master.json` — the new-standalone repo
  keeps its config and plugins instead of losing them.
- **Standalone repos are unchanged.** Without `master_url`, config
  and plugin policy keep living on the project's integration branch
  alongside the code, exactly as before.

### Working in a multi-repo hub

`bl` *does* record which code repo a ball came from: every ball
carries a `repo` field. But that field is create-anchored — it
captures where the ball was *filed*, usually but not always where
its code lives (bl-8994 tracks tightening this) — and the shared
`balls/tasks` branch is otherwise flat, so `bl prime` lists every
ball across every participating repo. A few conventions keep
per-repo work legible:

- **Claim from the clone whose code the ball touches.** Worktrees
  land under the current clone's `.balls-worktrees/`, so claiming
  from the wrong clone puts your edits in a tree that doesn't have
  the right source. Read the ball before claiming if it isn't
  obvious which repo it targets.
- **Filter on the `repo` field; don't reinvent it.** `repo` is
  auto-set at create from the creating clone's `origin` URL (else
  its repo path); `bl show` prints it and it's in `--json`. For
  per-repo triage, filter the `bl ready` output on `repo` rather
  than inventing a parallel convention to hand-duplicate a field
  that already exists and is already populated. A tag (`repo:api`,
  `repo:frontend`) is still a fine *manual override* for the cases
  where the auto-set value misleads — a cross-cutting ball, or one
  filed from the wrong clone — but reach for it as the exception,
  not the default.
- **Cross-cutting work: parent + per-repo children.** A change
  spanning two code repos becomes one umbrella ball (`-t epic`)
  plus one child per repo, each tagged with its target and claimed
  in its own clone.
- **The hub itself.** Most projects make the hub a bare repo that
  carries *only* the `balls/tasks` ref — no working tree, no code,
  just a remote every participant can push/fetch the orphan ref
  to. A real code repo can double as the hub when one of the
  participants is already the natural source of truth.

### Bridging to an external tracker (the proxy pattern)

A multi-repo project usually wants one external system (Jira,
Linear, GitHub Issues) as the human-facing record. The intended
shape with balls is to wire the plugin into **one** participating
clone — the **bridge** — and let the other code repos operate
through the shared state branch as a proxy. They never install the
plugin, never hold its credentials, and never run its sync.

```text
       ┌────────────────┐
       │    tracker     │   (Jira / GH Issues / Linear)
       └────────▲───────┘
                │  plugin sync (bidirectional)
       ┌────────┴───────┐
       │  bridge clone  │
       │ (plugin here)  │
       └────────▲───────┘
                │
       ┌────────┴───────┐
       │   state hub    │   shared balls/tasks
       └────▲───────▲───┘
            │       │
       ┌────┴───┐ ┌─┴──────┐
       │ repo B │ │ repo C │   no plugin — just push/fetch
       └────────┘ └────────┘
```

A ball filed from repo B lands on the shared branch; the bridge
sees it on its next sync and mirrors it outward. A ticket filed
externally arrives on the shared branch the same way and shows up
in `bl ready` in every participant. Repos B and C are unaware of
the tracker — they only ever talk to the state hub.

Why one bridge rather than per-repo plugins: external-tracker
plugins store credentials and run scheduled sync. Putting them on
every participant means N copies of the secret, N concurrent sync
races, and N places where mirroring policy ("should this ball go
out?") can drift. One bridge keeps secret, schedule, and policy in
one place.

The bridge's `BALLS_IDENTITY` should be stable — its name appears
on every mirrored ball and on every reply written back from the
tracker. Treat the bridge as a long-running participant, not a
transient claim.

## Creating Tasks

If you discover work that needs doing:

```
bl create "Fix the auth timeout" -p 1 -t bug --tag auth -d "Description here"
bl create "Add rate limiting" -p 2 --dep bl-a1b2 --tag api
bl create "Spike: retry strategy" --parent bl-a1b2        # child of an epic
```

- Priority: 1 (highest) to 4 (lowest)
- Types: `epic`, `task`, `bug`. The type is a label, not a behavior switch — `epic` does not gate anything, does not auto-link children, and imposes no workflow of its own. Use it as a visual/organizational marker for container work and wire real hierarchy with `--parent`.
- Parent: `--parent TASK_ID` establishes a hierarchical parent (e.g. an epic). Does not block; use `--dep` for that.
- Dependencies: `--dep TASK_ID` (blocks the new task until dep is closed, repeatable)
- Tags: `--tag NAME` (freeform labels, repeatable)

`bl create` prints the new bare task id (`bl-xxxx`) to stdout and nothing else on success — capture it directly in a shell variable when scripting.

## Dependencies and Links

```
bl dep add TASK_ID DEPENDS_ON_ID    # blocking dependency
bl dep rm TASK_ID DEPENDS_ON_ID     # remove dependency
bl dep tree                          # show full dependency graph
bl link add TASK_ID relates_to OTHER_ID   # non-blocking relationship
bl link add TASK_ID duplicates OTHER_ID
bl link add TASK_ID supersedes OTHER_ID
bl link add TASK_ID replies_to  OTHER_ID  # thread reply
bl link add TASK_ID gates       OTHER_ID  # post-review blocker — see below
```

Link types:

| Type | Enforced? | Blocks | Use for |
|---|---|---|---|
| `relates_to` | no | nothing | cross-reference |
| `duplicates` | no | nothing | mark a dup |
| `supersedes` | no | nothing | "this replaces that" |
| `replies_to` | no | nothing | threaded discussion |
| `gates` | **yes** | **close of the source task** | post-review audits (security, docs, coverage) — parent can't archive until gate targets close |

`gates` is the thing to reach for when "this task is done but I still need someone to audit it" is a hard requirement, not a convention. A parent with an open gate link will refuse `bl close` until the gate child is itself closed. Use `bl link rm PARENT gates CHILD` if you need to drop a gate explicitly (it leaves a commit trail). `dep` blocks claim of the child; `gates` blocks close of the parent — they are intentionally different primitives. See the README "Gates: post-review blockers" section for the full pitch and worked example.

### When to claim a gate target

If a task is the target of a `gates` link — i.e. some other task has `gates <this>` pointing at it — **do not claim it until that source parent is at status `review` or later.** Gate targets are audits of a near-final artifact. If you start the audit while the parent is still `open` or `in_progress`, you are reviewing code that does not exist yet; the work gets thrown away and redone once the parent actually lands on main.

Before claiming anything from `bl ready`, run `bl show <id>` and look at the `links` list. If another task has `gates` pointing at this one, also `bl show` the source parent: claim only when its status is `review`, `closed`, or otherwise past construction. `bl ready` does not filter these out for you — the rule is yours to apply.

(Exception: occasionally a user asks you to audit a design or spec *before* implementation. That's the user routing you explicitly — not something to infer from the queue.)

## Scripting with bl

When driving `bl` from a script or another agent, treat the following as the machine contract. Everything else in this doc is for humans reading task state — the contract below is what a loop can rely on.

**Machine-parseable stdout.** These commands accept `--json` and print a single JSON document to stdout (stderr is diagnostics only). Prefer `--json` over parsing the table output. Shapes differ by command:

| Command | Top-level shape |
|---|---|
| `bl prime --json` | object: `{identity, claimed, claimed_status, ready}` — `claimed` and `ready` are arrays of task objects |
| `bl ready --json` | bare array of task objects |
| `bl list --json` | bare array of task objects |
| `bl show ID --json` | object: `{task, children, closed_children, completion, dep_blocked, delivered_in_resolved, delivered_in_hint_stale}` — the task itself is under `task` |
| `bl dep tree --json` | bare array of root nodes, each with nested `children` |

Don't assume a `ready` key on `bl ready --json` just because `bl prime --json` has one — `bl ready` is a bare array. Iterate the top level directly.

**Commands that print a single token on success.** Capture stdout directly:

| Command | Stdout on success |
|---|---|
| `bl create ...` | bare task id, e.g. `bl-3f2a` |
| `bl claim TASK_ID` | absolute worktree path (or `claimed ID (no worktree)` with `--no-worktree`) |
| `bl link add A TYPE B` | `A TYPE B` confirmation line |

`bl review`, `bl close`, `bl update`, `bl drop`, `bl dep add/rm`, `bl init` print human status lines; don't grep them. Check the exit code (`0` = ok, non-zero = error with a message on stderr).

**Building a tree in one pass.** `--parent` plus `--dep` compose cleanly:

```
EPIC=$(bl create "Migrate auth layer" -t epic -p 1)
A=$(bl create "Extract token store" --parent "$EPIC")
B=$(bl create "Swap middleware"       --parent "$EPIC" --dep "$A")
bl link add "$EPIC" gates "$(bl create "Audit: rollback plan reviewed" --parent "$EPIC")"
```

Parents are hierarchy; deps are claim-time blockers; gates are close-time blockers on the source task. They do not interact — pick each one on its own merits.

**Identity.** Set `BALLS_IDENTITY` in the environment once, or pass `--as` on every command that accepts it (`prime`, `claim`, `update`). Scripts should prefer the env var so a single export covers the whole session.

**Exit codes and errors.** Every `bl` subcommand exits non-zero on failure and writes `error: <message>` to stderr. No command writes partial output on failure, so a non-zero exit means the stdout token you were about to capture is not there — always check `$?` before using it.

## Environment

| Variable | Purpose | Default |
|----------|---------|---------|
| `BALLS_IDENTITY` | Your worker identity | `$USER`, then `"unknown"` |

Set `BALLS_IDENTITY` in your environment or use `--as` on commands that accept it.

## Error Recovery

| Situation | Solution |
|-----------|----------|
| Merge conflict on `bl review` | Resolve in worktree, run `bl review` again |
| `bl close` errors "cannot close from within the worktree" | `cd` to the repo root, retry |
| Task claimed by someone else | Pick another from `bl ready` |
| Worktree in bad state | `bl drop TASK_ID --force` (loses uncommitted work) |
| Orphaned claims/worktrees | `bl repair --fix` |
| Stale `state branch records close for bl-xxxx...` warnings on `bl sync` (pre-0.3.8 gate closes, or abandoned deliveries) | `bl repair --forget-half-push <id>` per id, or `bl repair --forget-all-half-pushes` to retract every current one |
| Lost context mid-task | `bl prime` shows your claimed tasks |
| `bl` fails opaquely, or a repo's docs mention bl but commands error | `bl doctor` — read-only; names the exact cause and the fixing command |
