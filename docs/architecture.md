# balls — architecture

> **Status: FROZEN reference (2026-06-06).** This is the authoritative greenfield
> design that the implementation epic [bl-72a8] builds against — a frozen reference,
> not a moving ball. Extracted verbatim from working ball bl-2e26 (now archived) once
> the design stopped churning (§8/§15). The §15 revision log and the rationale ("the
> why") are preserved below.
>
> **Changing this doc:** corrections to already-built phases are tracked under bl-72a8;
> a *substantive design change* belongs in a new design ball plus a fresh entry in the
> §15 revision log — never a silent edit here. That discipline is what keeps "frozen"
> meaningful.

Consolidated spec: the single current source of truth (§0–§16). Assembled 2026-06-02 from the prior
body and 13 working notes, then completed as each topic ball under epic bl-b465 settled and edited
its § here directly. **Every design topic is now resolved and folded (§15); this is the finished
design, the basis for the implementation epic.** Implementation diverges from current balls — the
spec describes the greenfield target, not what ships today.

**REVISED 2026-06-05 (post-finalization, supersedes parts of bl-62bc/bl-0601).** Config-shadowing and
the trail/terminus/`operating` model are RETIRED. Config and tasks now ride SEPARATE branches —
`balls/config` (the landing) + `balls/tasks` (the store) — with `config/` and `tasks/` as top-level
folders always (reuse-safe, §2). Config NAMES the store via `tasks_branch` (§4); there is no trail,
no terminus, no `operating/` symlink, no config layering down a chain. **Landing config is the sole
authority** for what runs + where it syncs; ALL config is potential RCE and crosses only by `install`,
a **pure path-copy** (folder = mirror, file/glob = union — §6); a fresh landing is SEEDED at prime from
the app `default-config/` folder (no run-time defaults — §1); `sync` moves only the store. Touched
§0/§1/§2/§4/§6/§7/§8/§12/§13/§16. Corrections to already-built phases are tracked under bl-72a8.

## §0 Overview & core principles

balls is a git-native task tracker. State lives on TWO branches of a git repo — `balls/config` (the
**landing**: this checkout's config, path-derived, always local) and `balls/tasks` (the **store**:
the `tasks/<id>.md` data). Config NAMES the store (`tasks_branch`, §4); the store holds only data.
Both are default-named but reuse-safe (§2). Persistence is git, local-first. Base balls is the
smallest possible thing — it commits config edits to the landing and task-file edits to the store.
Everything that touches the world beyond those branches is a **plugin**. The shipped `tracker`
plugin is the only thing that talks to a remote (it syncs the store branch); the shipped
delivery/worktree plugin is the only thing that touches the project's code. Strip every plugin and
balls is a pure local task list.

Load-bearing principles (each enforced structurally, not by discipline):

- **Two branches, never `main`.** State lives only on the landing (config) and the store (tasks) —
  never on `main` or any project branch. "Nothing on main" is STRUCTURAL: base balls never opens
  the project repo, so core *cannot* leave a commit there. Config and tasks ride SEPARATE branches
  because they have OPPOSITE transport disciplines (§2/§6): config is single-owner, install-replaced
  (destructive, no merge); the store is shared, sync-merged (union, ff-only). One ref cannot carry
  both disciplines, so the split is what makes each safe — enforced structurally, not by discipline.
- **Unopinionated about workflow; no status field.** There is no `state`/`status` field at
  all (§3, bl-4778): status is a DERIVED view, not stored — `claimant` set ⇒ claimed, an unresolved
  claim-blocker ⇒ blocked (§10), a deleted file ⇒ closed (§9). Core enforces only the MEANING of its primitives, never a particular
sequence: `claim` refuses an already-claimed ball or an open claim-blocker (`!ready()`), `close` an
open close-blocker (`!closeable()`, §10), all reading structured fields. balls ships workflow
PRIMITIVES (occupancy + blockers), not a workflow; users, agents, and plugins compose them into
whatever process they want — "review before close," sign-offs, build gates are emergent (a gate
child plus a tag), never core rules. Destination semantics, not source.
- **Plugins are a sequence of binaries.** Invoked blocking, in the order the hook list gives, per
  op-phase. Non-zero exit aborts the op and rolls prior plugins back in reverse. That is the whole protocol.
- **Core knows two things about a plugin:** its name and its binary path. It never reads a plugin's
  config. Plugins coordinate only through core schema (task fields), never by sniffing each other.
- **LOCAL config is the sole authority — never a remote.** Core decides what runs and where it syncs
  from local config only: the landing's `config/balls.toml` (`tasks_branch`) + `config/plugins.toml`
  (the hook list, §4), with the per-machine XDG layer and CLI flags as local-trusted overrides (§4 read order). The
  landing is the durable, committed home of that authority; XDG and flags are your own machine, not a
  remote input. It changes only by user command (`install`, §6, or a direct landing/XDG edit) — never
  by automatic discovery. **All config is treated as potential RCE** and crosses into a landing only by
  the explicit copy `install` performs; reading or syncing a remote is free but NEVER authoritative. There are **no run-time defaults**: a fresh landing
  is SEEDED at prime by copying the app-level `default-config/` folder (§1/§12), so the trusted set
  (tracker, delivery, builtin plugins) are ordinary entries in the landing's list — not a magic
  carve-out. Swap that folder and you swap the default capability set: policy lives in config, not core.
- **Subtract before adding.** A new verb/state/field/flag is a smell; prefer an existing signal.
  Derive values rather than store them; make a component indifferent rather than teach it cases.

**Vocabulary** (see §2/§12): the **landing** is the always-present, path-derived local `balls/config`
branch holding this checkout's config — including `tasks_branch`, the one pointer that says where the
store is. The **store** is the `tasks/` on `tasks_branch` (default `balls/tasks`); config NAMES it,
the store holds only data. There is NO trail, terminus, or `operating/` indirection: config is read
from the landing, tasks are read/written on `tasks_branch`, and federation is many landings naming
ONE shared store branch (§12). "Stealth" is not a mode — it is simply a `tasks_branch` with no
configured remote (the store is local-only).

## §1 XDG layout

```
$XDG_CONFIG_HOME/balls/
  config.toml                          # user-level config layer (§4)
  default-config/                      # the SEED: prime copies this into a fresh landing (§12).
                                       #   install-default wires tracker + delivery + builtin plugins,
                                       #   so they are ordinary entries in the landing list — no
                                       #   runtime defaults. Replace this folder = replace the default
                                       #   capability set (policy in config, not core code).

$XDG_STATE_HOME/balls/
  plugins/<name>/<plugin-territory>/   # each plugin owns one subtree
    tracker/<pct-enc-remote>/          #   tracker: a clone tracking the store branch (.git/ tasks/)
    <delivery>/<local-path>/<id>/      #   delivery: the code worktree for task <id> (see §11).
                                       #     MIRRORS the invocation path (leading / stripped), NOT
                                       #     percent-encoded: this is a cargo build dir and rust-lld
                                       #     cannot write an output file under a `%` ancestor (bl-f3e4).

  clones/<pct-enc-local-path>/         # per-invocation-path binding
    binding.toml                       #   tracker remote (if any) + invocation_path + tasks_branch
    config/                            #   the LANDING — balls/config checkout (real, path-derived)
    tasks/                             #   the STORE — tasks_branch checkout; local (stealth) or a
                                       #     worktree tracking tracker/<remote>/ when remote-backed
    changes/<uuid>/                    #   in-flight CHANGE worktrees (core, ephemeral, one per op)
    log                                #   the unified op log: JSON-lines, balls-owned (§4/§6)
```

- URLs and paths are **percent-encoded, never hashed**, so directory names stay inspectable. The
  lone exception is the delivery code worktree (`<delivery>/<local-path>/<id>/`), which mirrors the
  invocation path verbatim: it is a cargo build dir and `rust-lld` cannot open an output file whose
  path contains a `%` (bl-f3e4). Mirroring is no less inspectable (§1's goal is *readable*, not
  *encoded*) and is always a valid path, since the invocation path already is one. Encoding is kept
  everywhere git data lives (clones, tracker), where nothing compiles and `%` is inert.
- `config/` and `tasks/` are SEPARATE checkouts of the two branches. There is NO `operating/` symlink
  and no terminus to resolve: config is read from `config/`, tasks from `tasks/` (named by
  `tasks_branch`, §4). If `tasks_branch` names the landing, the two checkouts coincide on one branch
  whose `config/` and `tasks/` folders simply both live there — same code path (§2).
- The CHANGE worktree (`changes/<uuid>/`) is core, uuid-named (nothing keys off the uuid), off the
  STORE for task ops and off the landing for `config`/`install` ops (§8). It is distinct from the
  delivery plugin's CODE worktree (§11), which lives in plugin territory and checks out the *project*
  repo.
- Two clones sharing one store remote share one `plugins/tracker/<remote>/` dir.
- **`log` is ONE unified per-clone op log** (not a per-plugin or per-op-phase tree): JSON-lines, one
  object per line `{ts, lvl, src, op, phase, msg}` with `src ∈ {core, <plugin>}`. balls owns the
  format — the source is a stamped FIELD, so you grep one source or read the whole sequence; metrics
  (§6) compose over it. It is local runtime state, gitignored, never on a committed branch (like
  `binding.toml`) — which the §4 "landing cannot be published" rule makes doubly correct. One object
  per line keeps concurrent appends from parallel agents atomic — every line is held at or below
  `PIPE_BUF` (4096 bytes) so a single `O_APPEND` write never interleaves with another agent's, and the
  one unbounded field (the §6 enveloped plugin-stderr `msg`) is truncated with a `…[truncated]` marker
  to honour that bound (bl-e6a0, §15). Stale-
  but-harmless like an orphan worktree: no core rotation/retention; prune is
  manual; the `log_level` knob (§4) limits volume instead. Scope is per-clone (per invocation-path),
  so one timeline covers both this clone's store *and* landing ops.

## §2 Branch layout — two branches, folder-namespaced

State rides TWO branches, each one job, each its own transport discipline (§0/§6):

```
balls/config   (the LANDING — path-derived, single-owner, install-transport)
  config/balls.toml                      # balls config (§4), incl. tasks_branch
  config/plugins.toml                    # the hook schedule (§4/§6): [hooks] <op>.<phase> = ordered name list
  config/plugins/<name>/...              # each plugin's config; balls never reads it
  config/plugins/bin/<name>              # local, gitignored: absolute symlink to this box's binary

balls/tasks    (the STORE — named by config.tasks_branch, shared, sync-transport)
  tasks/<id>.md                          # one file per task; <id> IS the filename basename
```

- **`config/` and `tasks/` are top-level folders, ALWAYS** — on every branch, in either role. This
  is what makes the split free: the code reads `config/` from the landing ref and `tasks/` from the
  `tasks_branch` ref with NO branching on whether those refs are equal. One-vs-two branches is just
  whether the two names coincide — not a code path. Point `tasks_branch` at the landing (or run both
  roles on one branch) and it Just Works: the two folders coexist. balls does NOT block on branch
  names.
- **Default-two.** `tasks_branch` defaults to a DISTINCT ref (`balls/tasks`), because two single-job
  branches are simplest and fewest code paths (§0). Reuse is legal but a deliberate choice with its
  own costs (§6): a config ref shared as another clone's store couples their cadences and carries
  baggage; a config ref shared AS config between clones CORRUPTS, having no merge.
- The landing branch is path-derived (`balls/config`) and is never named by config — you read config
  FROM it, so it cannot name where it lives. `tasks_branch` is the one indirection (§4): the single
  fixed point (the landing) plus the single pointer (to the store).
- **The store is materialized LAZILY — "a branch is a disk path" (bl-0a23).** Core founds the LANDING
  eagerly (it must read `config/` to know the chain and the `tasks_branch` name), but the store branch
  is laid down by `prime`'s fixpoint only AFTER the tracker has had its `prime/pre` pass. The primitive
  is `materialize(tasks_branch)`: ensure the named branch has a worktree on disk, founding a fresh
  orphan (`tasks/.gitkeep` root) **iff the ref is absent**. So an established remote branch the tracker
  just cloned into a local ref is CHECKED OUT (adopt — no divergent orphan to reset), and an orphan is
  founded only in the genuine no-remote bootstrap (§12). Founding the orphan eagerly was the
  unrelated-histories bug (bl-fa00): a fresh local root an established remote could not fast-forward
  onto. Materializing after the clone-in means that divergence is never CREATED.

No archive directory. Closed balls **delete** their `tasks/<id>.md`; history lives in
`git log` (`git log --diff-filter=D -- tasks/<id>.md`). The log is real, searchable CONTENT, always
read most-recent-down: `bl show <id>` reconstructs a dead ball from history (§9), and `bl list
-s closed/--all` reaches the dead set the same way. Closed tasks are older content, not tombstones —
the recency-resolution discipline (§ id generation) makes every id→content lookup one walk.

## §3 Task schema (one node type)

There is exactly one node type: **task**. `epic`/`issue`/`gate`/`bug` are *tags* with zero core
behavior; any "epic" rendering is emergent from the task having children (§10).

`tasks/<id>.md` — TOML frontmatter (fenced by `+++`), then a free-form markdown body. Core writes the
canonical serializer form: scalars first, each blocker a `[[blockers]]` table last.

```markdown
+++
title = "Refactor the foo system"
created = 1748357520                        # unix seconds; storage/transit is ALWAYS unix-time
updated = 1748443920                        # only display renders ISO-8601 (§9), never storage
claimant = "orionriver@gmail.com"           # occupancy: present ⇒ claimed, absent ⇒ unclaimed. NO status field
parent = "bl-1000"
priority = 2                                # optional; lower = higher priority; absent sorts LAST
tags = ["refactor", "infra"]

[[blockers]]                                # the relational primitive (§10), one table per edge
id = "bl-1100"
on = "claim"                                # can't be CLAIMED until bl-1100 resolves (dependency)

[[blockers]]
id = "bl-1200"
on = "close"                                # can't be CLOSED until bl-1200 resolves (gate)
+++

Free-form markdown body.
```

**Format (TOML everywhere).** Frontmatter and config (§4) are both TOML — one serializer, pure-Rust, no C
dependency. TOML earns its keep where humans hand-edit (config comments survive; `bl` rarely rewrites
config) and costs nothing where they don't (task files are `bl`-written, so block-form `[[blockers]]`
and comment-erasure on rewrite are immaterial). It exports deterministically and losslessly to JSON for
external tooling via `toml::Value → serde_json` — the i64 timestamps sidestep TOML's lone non-JSON type
(native datetime). Chosen over YAML (a C libyaml dependency) and JSON-everywhere (which loses config
comments).

- **No `id:` field.** The id is the filename basename of `tasks/<id>.md`, the sole source of truth
  (Model A — "id IS the path, not a field"). One token; `git log -- tasks/<id>.md` and `tasks/`
  greps work by id. (Rejected Model B — opaque-uuid filename + id field — because it forces an
  index for `bl show <id>`; never introduce an index unless it is the core basis of the app.)
- **No `state`/`status` field (bl-4778, RESOLVED).** A live ball's status is a DERIVED VIEW —
  computed on read by a short-circuit ladder, never stored, each value named for the VERB/question it
  answers:
  1. `claimant` set ⇒ **claimed** (someone holds it). Claim-blockers are NOT evaluated here — once a
     ball is claimed, whether it *could* have been claimed is moot.
  2. else an unresolved claim-blocker ⇒ **blocked** (not startable yet).
  3. else ⇒ **ready** — claimable now. This is exactly the `ready()` predicate (§10); "ready" is the
     question we ask of a ball, so it is the word shown, not "open".

  Exactly THREE live-ball states (closed is not a state — the file is gone, §9). **"blocked" means claim-blockers ONLY.** A close-blocker (gate/PR) yields NO
  status — a claimed ball with an open close-gate is just **claimed**. This is the same insight that
  abolished `review`: "in review" was never a functional state, only "claimed, with a close-blocker that core enforces at close" (§9/§10). A stored status field would have ZERO core behavior (no
  transition matrix; `ready()` never reads it), making it indistinguishable from a single-valued tag
  — so it folds away. Non-deriving human intent (`deferred`, `icebox`, `triage`) is an ordinary `tag`
  (§3), not a status. `bl list`'s status column RENDERS the ladder; nothing writes it. **Two
  projections, one source (bl-d074).** The DEFAULT human render (`bl show`/`list`) freely
  paints DERIVED columns — the status ladder, the tree, ISO-8601 dates — none of them stored. `--json`
  is the orthogonal **bedrock** projection: the lossless mirror of stored frontmatter ONLY, never a
  derived field, so "show me what's actually there" stays uncontaminated and round-trip-safe. A machine
  integrator therefore reads bedrock `--json` (`claimant` + `blockers` — already present) and runs the
  same short ladder core runs; no stored `status` is needed, so a denormalizing default `status` plugin
  folds away (bl-d074 RESOLVED by subtraction, §15). Old balls' 6-variant status enum collapses to
  `claimant` + `blockers` + tags, status computed on read. (A team wanting a stored, ordered pipeline —
  or the gate/PR `in_review` distinction core deliberately folds into "claimed" — opts in OUTSIDE core:
  an unknown `state:` key is preserved on writeback per the last bullet, read by their own display
  plugin — severable, never a core field.)
- **`claimant`** — the OCCUPANCY field and its SOLE source of truth: absent ⇒ unclaimed, present ⇒
  claimed. The one hardcoded guard (§0 — claim refuses an already-claimed ball) reads `claimant`,
  the structured field, so claim-correctness needs no status vocabulary at all. `claim` sets it,
  `unclaim`/`close` clear it (close by deleting the whole file).
- **`parent`** — containment/tree pointer only; scanned for display (`bl show` tree), never for
  enforcement. Arbitrary depth.
- **`priority`** (optional int) — the one ordering input. Unlike `status` (folded because `ready()`
  never reads it), priority has genuine core behavior: `bl list` ORDERS the ready set by it, and no
  other field can derive an order — so it does NOT pass the "zero core behavior ⇒ fold" test and
  stays a core field. Lower = higher priority; **absent sorts LAST** (a no-priority ball is lower
  than any priority). Ordering is display-only — never part of the `ready()` predicate (§10).
- **`blockers: [{id, on}]`** — the relational primitive (§10), stored on the BLOCKED task. `on` is
  ANY op name (`claim`/`close`/`update`/`unclaim`/…): the op of *this* task whose `.pre` is rejected
  while `id` is unresolved (`.pre` is implicit — blocking is always a pre-op rejection). NOT an enum —
  `claim` (dependency) and `close` (gate) are merely the two cases with create-time sugar (§10); the
  primitive itself gates any op. Subsumes the old `deps`/`gates` link types — they were one edge
  parameterized by `on`. Pure-metadata links (`relates_to`, `supersedes`, `duplicates`, `replies_to`)
  remain core metadata with no enforcement.
- **`created`/`updated` are unix-time (i64 seconds)** — storage and transit are ALWAYS unix-time; only
  display renders ISO-8601 (§9). A timestamp is just an int, so the storage layer needs no date library,
  the value is timezone-unambiguous and numerically sortable, and the TOML→JSON export stays lossless.
  `claimant`, `priority`, `tags` optional. Unknown keys preserved on writeback (this is the opt-in seam
  for a team's own `state:`/pipeline field). Terminal ops delete the file (§9).

## §4 Config schema

Config's durable, committed home is the landing (`balls/config`); it is NEVER read from the store and
NEVER layered down a trail (there is no trail — §12). The EFFECTIVE config is the landing overlaid by
two local-trusted layers — the per-machine XDG file and CLI flags — and that whole LOCAL stack is the
sole authority for what runs and where it syncs (§0): no remote is ever authoritative. A center's
config reaches you only by `install` copying it INTO the landing, where it becomes local (§6).
**Read layers, innermost wins:**
1. CLI flags
2. `$XDG_CONFIG_HOME/balls/config.toml` (per-machine, local-trusted)
3. `config/balls.toml` on the landing
4. Built-in serde fallback (absent fields only)

**Defaults are seeded, not run-time.** A fresh landing is populated at prime by copying the app-level
`default-config/` folder (§1/§12) — so the landing's `config/balls.toml` and plugin wiring (tracker,
delivery, builtin plugins) come from the SEED, as ordinary committed entries. The "built-in" layer 4
is only the serde fallback for a field no layer set. There is no run-time default plugin set and no
magic capability: everything that runs is a literal entry the seed (or a later `install`) wrote.

XDG and the seed are LOCAL trust — your own machine — never a remote input. A center's config reaches
you ONLY by `bl install` (§6) copying it into your landing, with consent. **This retires the old §4
trail-layer:** config values no longer shadow down a chain of checkouts; the adoption happens ONCE, at
explicit install, materialized into the landing. **All config is treated as potential RCE** (§0):
`install` IS the consent boundary; auto-layering leaked around it (a shadowed `tasks_branch` could
silently redirect where you write — config is not as inert as "data, not code" implied).

**Merge semantics:** scalar/object fields — innermost layer fully replaces outer. List fields —
bare `<field>` = full replacement; compose with `<field>_prepend` / `<field>_append` / `<field>_ban`.
The `[hooks]` lists in `config/plugins.toml` (§6) ARE list fields, layered the same way — a landing's
schedule composes with an XDG `_prepend`/`_append`/`_ban`, and `bl install` copies the file like any
other config (never inherited down a trail — there is none, §12). An absent/empty hook list = run nothing.

**The landing is single-owner — and balls cannot publish it.** Config's transport is a path-copy
mirror (§6 install), which has NO cross-writer merge, so two writers to one config branch clobber.
Single-ownership is therefore STRUCTURAL, not disciplinary: the only component that pushes is the
tracker, and it pushes ONLY `tasks_branch` (§13) — nothing in balls ever puts the landing in a push
refspec, so `balls/config` physically cannot leave the box through balls. The landing is path-derived
per-checkout (a different fixed point per clone) and is NEVER a sync target (§13). The one residue
balls cannot police is a human running raw `git push origin balls/config` by hand — outside balls'
surface, the same way `rm -rf .balls` is. Sharing a config branch between clones corrupts; only the
STORE (`tasks_branch`) is shareable, because only it is sync-merged (§6/§12).

**Built-in fields:**
- `tasks_branch` (string, default `"balls/tasks"`) — the branch whose `tasks/` is this checkout's
  store. The one config→store indirection ("config tells us where the tasks are"). A local-only value
  is stealth; the remote that backs it is the tracker's own config (§0 — core names no remote). The
  landing branch itself is path-derived (`balls/config`), NOT a config field (bootstrap fixed point).
- `log_level` (string, default `"info"`) — the single threshold for the unified op log (§1/§6),
  applied at WRITE time so it gates BOTH file persistence and terminal echo (a line below threshold is
  never emitted anywhere). A serde-default scalar like `tasks_branch` — NOT a `default-config/` seed
  entry and NOT a "run-time default" carve-out: the seed is for capability *sets* (the plugin chain),
  the layer-4 serde fallback is exactly "for a field no layer set." Read order is the normal §4 stack,
  so `--log-level` (layer 1) overrides for one run. Default mapping: core narrates mutating-op
  lifecycle at `info` and read-op narration (`show`/`list`) at `debug`, so default-`info`
  keeps read chatter out of the log; plugin-enveloped stderr is `info`.

The id scheme is deliberately NOT a config field — it is fixed (§ id generation): a team wanting a
different scheme supplies a `create/pre` plugin, not a config knob.

No state-related config exists at all (bl-4778, RESOLVED): **no `default_state`, no `states` vocab
list, no per-op state-target knob.** There is no `state` field for them to configure (§3) — status
is derived from `claimant`/`blockers`/file-existence, and human intent is a `tag`. The whole cluster
was rejected as the §0 "new field is a smell": claim writes no status, so there is no target to seed
or override; a team wanting an ordered pipeline opts in outside core (an unknown `state:` key + a
display plugin, §3). The plugin chain, by contrast, **IS a config list** — `config/plugins.toml`'s
`[hooks]` table (§6), layered like every other field (§4). The filesystem holds only the local,
gitignored `bin/<name>` binary symlinks; the *schedule* — which plugin runs in which op-phase, in what
order — is config, not a directory tree (a list is sortable; a directory needs `NN-` prefixes to fake it).

## §5 Commit-message protocol

Every change-attempt commit is `subject / body / trailer-block`, where the trailer block is a
**standard git trailer paragraph** (last blank-line-separated paragraph, `key: value` lines,
parsed via `git interpret-trailers --parse` — no hand-rolled parser; coexists with
`Co-Authored-By:`):

```
<subject — always the ball's title (no override)>

<free-form body, optional — the `-m` narration>

bl-protocol: 1
bl-op: close
bl-id: bl-1234
bl-actor: orionriver@gmail.com
```

- Tokens are lower-kebab (`[a-z0-9-]`, git's trailer grammar). The subject is ALWAYS the ball title
  (there is no override) so `git log` readers never see balls-flavored subjects; the optional `-m`
  text is the free body (narration), and `--body` is the ball's own markdown body, NOT a commit note.
- **Namespacing:** every key is namespaced by owner. `bl-` is RESERVED to core (plugins may not
  emit `bl-*`); plugins prefix with their own name (`jira-id`, `github-url`).
- balls always writes `bl-protocol`, `bl-op`, `bl-actor`; `bl-id` on every per-task op
  (`create`/`claim`/`unclaim`/`update`/`close`), absent on the checkout-scoped ops
  (`prime`/`sync`/`install`/`config`) which name no single ball. **No `bl-from-state`/`bl-to-state`
  trailers** (bl-4778) — there is no status field to transition; `bl-op` already names the op,
  and the `claimant` change
  rides as an ordinary frontmatter diff.
- Repeated key → list (git-native; no comma-splitting). Unknown keys are never dropped — parsed
  into `metadata` and forwarded to plugins on the invoke wire (§7).

## §6 Plugin contract & dispatch

A plugin is a single binary, dispatched **subprocess-uniform** — no in-process path, no privileged
plugins. The shipped `tracker` and delivery plugins are fully separate binaries, in-repo only as
default capabilities + reference implementations, invoked identically to any third party. Core
spawns every plugin directly as `<bin> <op> <phase>` — uniformity is carried by that spawn
contract, not by a dispatch verb (there is none; bl-587f, §15). Hand-testing a wiring is just
running the binary by hand with the same argv.

```
<bin> protocol
  → stdout (JSON): { protocol: <version(s)>, ops: [...] }   # self-description; balls never persists it
  → exit 0

<bin> <op> <phase>
  cwd:    the CHANGE worktree (mutating ops) or the relevant checkout (reads: store / landing)
  env:    BALLS_PROTOCOL=1, BALLS_PLUGIN_NAME=<name>, BALLS_PLUGIN_DEPTH=<n>
  stdin:  payload (§7)
  stdout: the plugin's USER-FACING channel — balls forwards it to the invoker's stdout verbatim
          and PARSES NOTHING back into state (no return channel; see §7). A plugin that produces a
          user-relevant value (the delivery plugin's worktree path, a forge PR URL) PRINTS IT HERE;
          core neither computes nor consumes it. "claim prints the worktree path" is exactly this —
          the delivery plugin printing, not core knowing the path.
  stderr: the plugin's diagnostic channel. The plugin stays DUMB — it writes raw stderr and is told
          nothing about where it lands (no BALLS_LOG_DIR; a new env is a §0 smell). balls pipes the
          child's stderr and ENVELOPES each line as a JSON-lines record into clones/<enc>/log
          (`src=<name>`, the op/phase, `lvl=info`), interleaved with core's own lifecycle records.
          Structured (non-diagnostic) artifacts a plugin wants go to its OWN territory
          (plugins/<name>/, §1), derived from BALLS_PLUGIN_NAME — never the shared log.
  exit:   0 = ok; non-zero = abort + roll prior plugins back in reverse. On a non-zero exit core
          additionally emits an `error` record (op/phase/name/exit) so the failure locus survives any
          `log_level` threshold (§4) even when the plugin's own info-level stderr is filtered out.
```

**Reads may dispatch plugins too.** Dispatch is op-uniform — there is no rule that only mutating ops
invoke plugins. A READ op (`show`/`list`) carries no seal and no `pre`/`post` split, so it dispatches a
SINGLE phase: core runs the named plugin (cwd = the relevant checkout, §7 wire minus the task-op
fields), forwards its stdout into the HUMAN render, and parses nothing back (§7 — still no return
channel). This is how `bl show` surfaces a value only the plugin can compute (the delivery worktree
path, §11): core cannot derive it (it never opens the project repo, §11) so it asks the owner. `--json`
never dispatches — it stays the lossless mirror of stored frontmatter (§9), so a read-op plugin's
output reaches the human projection alone, never the machine contract. A read dispatch that fails is
non-fatal: the read still renders, minus the plugin's line (a read mutates nothing, so there is nothing
to roll back).

**Metrics are a query, not core state.** balls stores and emits no metrics: the unified `log` (§1) is
the event stream — every op/phase/plugin record is timestamped — and the §5 commit trailers are the
durable history. Counters/timing compose over those by `jq`/parse, or a `*.post` plugin observes the
lifecycle and writes to its own territory. There is no metric seam to add: the hook list + this
dispatch + the §7 payload IS the subscription, and a plugin times an op by stamping its own clock
across the `pre`/`post` it already runs. No core storage, ever.

**The hook list is config.** `config/plugins.toml`'s `[hooks]` table on the landing (§2) is the single
source of truth for wiring: a `<op>.<phase>` key maps to an ORDERED LIST of plugin names — listed = run
this plugin in this op-phase; **list position = run order** (the last name runs last). An absent key or
empty list = run nothing (the general path with no entries, not a special case). It is layered + merged
exactly like the rest of config (§4) — a center's schedule composes with an XDG `_prepend`/`_append`/
`_ban` and travels by `bl install` — so there is NO parallel filesystem-registry mechanism; ordering is
a list property, not an `NN-` filename convention faking one.

```toml
[hooks]
"sync.pre"     = ["tracker"]                  # import remote state first
"prime.pre"    = ["tracker"]
"install.pre"  = ["tracker"]                  # fetch the center's config to adopt (§13 prime --install)
"prime.post"   = ["bl-delivery", "tracker"]   # re-materialize still-claimed worktrees + print their paths, then settle store content (fetch-ff + push)
"claim.post"   = ["bl-delivery", "tracker"]   # worktree (prints its path), then the push (tracker last)
"unclaim.post" = ["bl-delivery", "tracker"]
"show"         = ["bl-delivery"]              # read-op (single phase): fold the worktree path into the human render (§11)
"close.pre"    = ["bl-delivery"]              # deliver (squash) before the seal
"close.post"   = ["bl-delivery", "tracker"]   # teardown, then push
"create.post"  = ["tracker"]
"update.post"  = ["tracker"]
```

Schedule (committed text) and binary (local symlink) split cleanly:
- COMMITTED (travels with the branch / `bl install`): the `[hooks]` list names plugins by NAME — pure
  text, portable verbatim, valid in stealth and federation regardless of where the checkout sits.
- LOCAL (never committed, gitignored): `config/plugins/bin/<name>` is an ABSOLUTE symlink → this
  machine's binary. Installing a plugin drops this symlink; dispatch resolves a hooked NAME to it. A
  dangling/absent `bin/<name>` = a clean "plugin referenced but not installed here" error.

**`bl install` — copy a committed path between branches.** `bl install <path> --from <ref> --to <ref>`
makes `<ref-to>/<path>` byte-identical to `<ref-from>/<path>`, touching NOTHING outside `<path>`,
committed atomically to `--to`. Defaults: `--to` the landing, `--from` the configured upstream; **bare
`bl install` (no path) = `config/` EXCLUDING `tasks/`** (the recommended bundle — `balls.toml` +
`plugins.toml` + plugin config, never the store). `<ref>` is any synced repo/branch. There is **no object enum and no
merge-vs-replace logic** — install is path-copy, and the path's *shape* decides the semantics:

- **Folder path = MIRROR (deletions propagate).** `bl install tasks` makes the destination's `tasks/`
  identical to the source's — entries the source lacks are REMOVED (wipe-and-replace, rsync-`--delete`,
  NOT `cp` semantics). `install config` / `install plugins.toml` / `install plugins/<name>` each
  replace exactly that subtree or file. This is how a close (a file deletion) PROPAGATES through `install tasks` —
  the resurrection problem is dissolved by addressing, no tombstone mechanism needed.
- **File / glob path = UNION (additive, source-wins on overlap).** `bl install tasks/*` copies each
  source file in and leaves the destination's other files alone; `bl install tasks/bl-1234.md` ports
  one task. A same-`id` is OVERWRITTEN, NOT conflicted — there is no conflict detection; git is the
  recovery net. A deliberate sharp edge: `install tasks` is a hard wipe, `install tasks/*` a union; the
  skill doc states it. (This is the one counterintuitive case — folder ≠ `cp`.) Mitigation is honesty,
  not a guard: install commits, so every deletion is git-recoverable, and install PRINTS its change
  summary (`N added / M deleted`) on stdout — the blast radius is visible before you trust it, and
  `git diff` reviews it after. No confirmation flag (a new flag is a smell, §0); the commit IS the undo.
- **Siblings are never touched.** install builds its commit on top of `--to`'s current tip, swapping
  only `<path>` — it NEVER rebuilds the tip from the source's whole tree and NEVER resets the ref. So
  `install config` can never eat a co-resident `tasks/` (if `tasks_branch` names the same branch, §2):
  forbidden, not merely discouraged. More-specific paths are less destructive — scope the blast radius.
- **Committed tree only, never `bin/`.** A path-copy of the committed tree cannot include `bin/<name>`
  (gitignored, not in the tree), so publishing to another repo (`--from landing --to <center>`) ships a
  *recommendation* (a dangling `bin/` the recipient resolves locally), never runnable code. Adopting
  (`--to landing`) and publishing are the same verb, reversed. Re-homing the store = a `tasks_branch`
  edit + `install tasks` (mirror, to an empty home) or `install tasks/*` (union, into a populated one),
  §12.
- **Local binary resolution.** When `--to` is the local landing, install resolves each named plugin's
  `bin/<name>` against this machine (PATH or explicit `--bin <name>=<path>`) and validates it against
  the live `<plugin> protocol` (refuses to link an op or protocol version the binary doesn't declare).
  Still validated per binary, still user-in-the-loop: the consent-gated path for federated onboarding
  (§12). (`bl install` subsumes the older `bl plugin install <name> <path>` and `--from <branch>`
  spellings.)

- **Invocation-tree cap (the runaway backstop):** every nested `bl` — a plugin shelling back, an op
  triggering another op, a clone spawning a clone — runs as a descendant process and inherits one
  depth odometer (`BALLS_PLUGIN_DEPTH`, bumped once per nested invocation whatever its SOURCE; plugin
  recursion is just ONE dimension of it, clone-spawn another). A single built-in cap bounds the
  odometer along any root-to-leaf path. **Crossing the cap ABORTS the op — fail, not silent:** core
  rolls the run plugins back in reverse order (§8/§14) and emits a diagnostic naming the op/plugin
  chain that overran, so the loop SURFACES instead of hiding. (The retired disposition — "run
  PLUGIN-FREE at the cap, suppressed not refused" — was the worst option: it converted a runaway into
  quiet wrong-behavior, the offending plugin getting no signal and the op silently running without the
  plugins it expected.) The cap is the general failback for UNBOUNDED RECURSION in any dimension: a
  plugin wired on op X whose handler runs `bl X` (self-retrigger — the common bug), recursive
  clone-spawning, and mutual `plugin1`↔`plugin2` loops are all the one odometer crossing the one cap.
  A BOUNDED chain never nears it — forge `claim.post` running `bl create` for the gate child goes 1
  deep and never re-triggers its own op. There is no hatch to re-enable plugins on a nested call (it
  would let a runaway defeat its own backstop). A plugin SHOULD NOT re-trigger its own op; §0 already
  bars a plugin from depending on another plugin's presence (the mutual half). Finer-grained per-op
  controls can layer ON TOP; this cap is the failback under them. The cap is a FOOTGUN-GUARD, not a
  security boundary: the odometer rides the child environment (`BALLS_PLUGIN_DEPTH`), and a plugin —
  already arbitrary local code — fully controls the environment of any `bl` it spawns and can reset
  it. That is accepted: the cap exists to surface accidental cascades, and no control may rely on it
  as a sandbox or trust boundary.
- **Snapshot:** an op reads the effective `[hooks]` schedule at op-start and uses that frozen set;
  an install landing mid-op affects only the next op.
- **Reads are not special-cased.** Every op (incl. `show`/`list`) has a hook key, and the default
  schedule USES one: `show` lists the delivery plugin (the §11 worktree-path fold into the human
  render — the bl-0af4 read-op dispatch). The other reads (`list`, `dep-tree`) stay plugin-free in
  PRACTICE only because nothing is listed for them by default.

## §7 Plugin wire payloads

Plugins get meaning directly on the wire — content + intent, not hashes to reverse-engineer.
**There is no return channel:** a plugin contributes by EDITING THE CHANGE WORKTREE (the ball file,
frontmatter, a `git mv`), never by printing values for balls to merge. stdout is diagnostics. Two
plugins writing the same field is two filesystem writes — last writer wins, where "last" is the
hook-list order; balls neither arbitrates nor tracks ownership.

**pre payload (stdin):** `protocol`, `op`/`phase`, `plugin_name`, `actor`, `binding`
(`{ remote, tasks_branch, store, landing, invocation_path }` — `store`/`landing` are the two checkout
paths (§1), `tasks_branch` names the store branch (§4), `invocation_path` is where `bl` was invoked,
the project-repo root the delivery plugin needs, §11), `command` (`op` + intended `body_change`),
`current_state` (`null` on create). There is **no wire-carried field changeset**: the op's field-level
diff has one authoritative home — the change worktree plus the op-start state to diff against — and a
plugin reads it there, never from a second wire description (derive-don't-store, §14; bl-3bfd, §15).
The id is NOT on the pre wire (it is not sealed
yet — a reassigning plugin reads it from the single staged `tasks/*.md`).

**post payload (stdin):** same plus `commit`/`previous_commit`, the final `command`, `metadata`
(parsed from the §5 trailer block, incl. the now-sealed `bl-id`), and `previous_state` (the op-start
ball — what `pre` saw as `current_state`; `null` on create). There is **no post `current_state`**: a
`post` reactor MUST NOT mutate the sealed ball (§14) and derives the LANDED ball from git — the
`commit` it is handed plus `git show` — never the wire (derive-don't-store, §14; bl-667e, §15). So the
after-state is a read, not a payload field; `current_state` exists on the `pre` wire alone.

**rollback payload:** the shape of the op being undone plus `rolling_back: pre|post`; once the op
has SEALED, every rollback — whichever phase it undoes — also carries the post facts
(`commit`/`previous_commit`/`metadata`): §14's id rule ("post/rollback from the sealed §5 trailer")
makes no phase split, and the post-abort change worktree is clean, so a pre-phase rollback starved of
the trailer has no staged task file left to re-derive its id from (bl-430e). The plugin
tracks its own intermediate state (§11 rollback is the worked derived-state example; general rule §14).

## §8 Op lifecycle

balls runs TWO families of op, and they are deliberately NOT forced into one shape. Don't read the
sequence below as universal — it is the **task-op** shape; the rest inherit what generalizes and no
more.

**Task ops — the symmetric family** (`create`/`claim`/`update`/`close`): **balls authors a base
change, an ordered plugin chain acts on it, balls seals it, plugins react.** The boundary is the SEAL —
`commit + integrate`, atomic: **pre** modifiers shape what gets sealed (the record isn't fixed yet);
**post** reactors act on the now-landed record and MUST NOT mutate the ball (it is sealed). One commit
per op, sealed to the STORE. This is the canonical sequence (below), and it is fully symmetric across
the five verbs — they differ only in which fields the base change stages.

**Everything else inherits partially — a spectrum, not the same shape:**
- **`config` / `install`** keep the sealing shape but seal to the LANDING, not the store; `install`'s
  "base change" is a path-copy (§6), not a staged frontmatter edit, and it carries none of the
  task-shaped wire fields (`current_state`, `body_change`). Symmetric in skeleton, different in
  target and mechanics.
- **`sync` / `prime`** (§13) author NO diff at all, so there is no change worktree and
  no core seal. They inherit only what generalizes — the `pre`→`post` hook spine in hook-list order,
  reverse-order rollback, and the §7 wire minus its task fields — and do their own work where the seal
  would be: `sync`'s integration is the tracker's fetch+ff, owned by a PLUGIN not core (remote-talk is
  plugin-exclusive, §0/§13); `prime` orchestrates syncs + substrate.
  Their hooks run against the live store/landing checkout directly. **`prime` runs a FIXPOINT, not a
  single pre→post (bl-0a23):** core loops `prime/pre` then `materialize` until the configured
  `tasks_branch` stops moving, THEN runs `prime/post` once — so an established remote branch the
  `pre` tracker clones in is checked out before any orphan is founded. The loop is CORE's, its signal
  the config branch core owns (no §7 return channel); `pre` runs with cwd = the landing (the store may
  not be materialized yet), `post` with cwd = the now-materialized store. It is bounded by the §6
  invocation-tree cap and converges in one pass when the dial holds (the common case).

The canonical task-op sequence (verb-agnostic):

1. **balls makes the place to work + authors the base** — the CHANGE worktree (off the STORE for task
   ops, off the landing for `config`/`install` ops),
   into which balls STAGES the op's base change (create stages a provisional `tasks/<id>.md` with a
   default-generated id; claim stages `claimant`). balls goes first; plugins extend/override.
   (For `prime`'s bootstrap-on-miss it makes the landing repo instead — the substrate-creation phase;
   `prime` is a normal op, not a dispatch exception.)
2. **pre modifiers run in hook-list order** — the §7 wire (current state + intent). They edit the shared
   worktree (rename the ball file to reassign an id, edit frontmatter) or REJECT. They see each
   other's cumulative FILE state, never each other's commits (§7) — there are no intermediate commits.
3. **balls SEALS — commit + integrate, atomically** — validate, commit the worktree with the §5
   trailer (re-reading the tree to learn the final id/state), and integrate it onto its target branch
   (the STORE for task ops, the landing for `config`/`install`) as ONE act. *This is the pre/post
   boundary; after it the record is durably on that branch.*
4. **post reactors run in hook-list order** — the §7 wire with the sealed `bl-id`/state. They act on the
   landed record (tracker pushes the store; the delivery plugin acts on the project repo) but DO NOT edit
   the ball — anything that had to live on the ball was written in pre; post-only values are DERIVED,
   never written back (§11 `delivered_in`). The outermost irreversible effect (the push) sorts LAST.
5. **teardown** — balls removes the change worktree (its content is already integrated).
6. **failure / rollback (§14)** — any non-zero `pre`/`post` exit aborts: core calls `rollback` on each
   already-run plugin in REVERSE order, then UN-SEALS its own change (discard the un-sealed worktree on
   a pre abort; `git reset` the target branch on a post abort — local, reversible, nothing core-pushed).
   Each plugin's rollback decides what undoing means; persistence-through-abort is a plugin choosing a
   no-op rollback (§14), not a core carve-out.

## §9 Verbs

Deliverable lifecycle verbs: **`create`, `claim`, `unclaim`, `update`, `close`.**
There is **no `review` verb** — see "close" below.

Read verbs (no seal, no change worktree — hook dirs only, §13): **`show`, `list`, `dep-tree`.**
(There is **no `ready` verb** — it folded into `bl list --status ready`; see below and §10.) They
author no ball-file diff; their whole contribution is what the hook chain prints (§7). `--json` on any
read verb is the lossless **bedrock** projection — raw stored fields only, no derived value (the
round-trippable "what's actually there", §3); the default human render is the orthogonal projection
that carries the derived columns (the status ladder, the tree, ISO-8601 dates).

**`list`** is the SINGLE listing verb (the old `ready` folded in — fewer verbs, composable filters).
It defaults to the live/open set (current `tasks/*.md`) and orders by `priority` ascending (absent
last), then `created` ascending — the one ordering input (§3) drives every listing, so ready's
ordering was never special. Filters COMPOSE (AND):
- `-s` / `--status ready|blocked|claimed|closed` — the §3 status ladder as a single predicate axis (a
  space-separated value, not a bespoke boolean). The three LIVE rungs filter the live ladder;
  `bl list -s ready` IS the old `bl ready`: live + unclaimed + every claim-blocker resolved (§10), in
  the ordering above. The terminal rung `closed` has no live badge, so it INFERS the dead-set reach —
  the live/history split is an implementation detail, not a second flag idiom.
- `--all` — reach BOTH sets (live + dead). Closed balls are not gone, they are older content (§2):
  reconstructed from history (deleted `tasks/*.md`), recovered most-recent-down. `-s closed` = dead
  only; `--all` = live + dead. (`-s closed` and `--all` both pick the dead set, so they don't combine.)
- date/range, `--tag`, and text filters — applied uniformly to the (possibly history-served) set;
  date filters read the stored `created`/`updated` (and, for a dead ball, its deletion-commit date).

**`show <id>`** resolves by RECENCY (the unifying discipline, § id generation): live `tasks/<id>.md`
first; on a miss it walks `balls/tasks` history newest→oldest and reconstructs from the most recent
commit whose tree still holds `tasks/<id>.md`, stopping at the FIRST hit (a dead ball renders with its
retirement derived from the deletion's `bl-op:` trailer, §5). Closed tasks are searchable content, not
tombstones — the same most-recent-down walk that `list -s closed/--all` uses.

Checkout-lifecycle verbs (the checkout itself, not a ball): **`prime`, `sync`, `install`** (§13, §6).
**There is no `init` verb** — it retired into idempotent `prime` (§12): founding is just `prime`'s
bootstrap-on-miss path. `prime` makes the checkout ready (substrate + onboarding + worktree
re-materialization, seeding a fresh landing from the app `default-config/` folder — §12); `sync` keeps
it current (data only); `install` copies a committed path between branches (§6 — config/plugins adopt
or publish; the recommended bundle is `config/` minus `tasks/`).

**`create`** (op `create`; no prior state): balls generates a default-scheme id (§ id generation)
and stages `tasks/<id>.md` (title, timestamps, optional `parent`/`priority` (`-p`)/`tags`, plus any
`blockers` edges spelled out by `--blocks`/`--needs` — §10; `--parent` adds containment only, no
blocker; no status field is written, §3). A `create/pre`
plugin may reject or *reassign*
the id by `git mv`-ing the single staged `tasks/*.md` (it discovers the current name by reading
that file, never hardcoding — so reassigns compose under hook-list order). balls validates (exactly one
new `tasks/*.md`; regex-valid; no collision) and commits.

**`claim`** (acquire occupancy; core's guards refuse a ball whose `claimant` is already set, or whose claim-blockers are unresolved — `!ready()`, §10): stage `claimant`, bump `updated` — the ONLY field it writes. There is no status to set:
"claimed" is the derived view of `claimant` (§3), so claim stores the one occupancy fact in the one
field (the only field CORE writes). `claim.post`: the delivery plugin materializes the code worktree and
PRINTS its path on stdout (§11), which balls forwards verbatim — the natural product of the verb, the way
`create` prints the id. The path is not stored anywhere: it is a pure function of `(binding, id, claimant)`
and already a local git fact (`git worktree list`), so balls neither computes nor records it (§11;
derive-don't-store).

**`unclaim`** (release occupancy): clear `claimant` (symmetric with claim — the only CORE field
touched). `*.post`: the delivery plugin releases the code worktree. There is nothing to unstage — the
worktree path was never a field.

**`update`** (op `update`): the generic field/body edit — it overwrites EVERY ball field, so there is
no create-only split. Retitle (`--title`), rewrite the markdown body (`--body`), set or clear the
`parent` (`--parent`/`--no-parent`) and `priority` (`-p`/`--no-priority`) scalars, add or drop a
`tag` (`-t`/`--no-tag`), set or remove a preserved extra (`key=value`/`key=`), and add or unlink its
own `blockers` (`--needs`/`--no-needs`, the §10 in-band edge edit). The lone create-only flag is the
reciprocal `--blocks` (an edge naming this task on ANOTHER), since that is not this task's own field.
A blocker that really blocks must be removable in-band — no store-file surgery — and the same holds
for a stale parent, a wrong title, or a leftover scalar. (No status to set — that field doesn't
exist, §3; a team's opt-in
`state:` key, being an unknown preserved field, rides through `update` like any other.)
**`--edit`** is the HUMAN projection of the same update (bl-e196): render the stored `tasks/<id>.md`
to a temp buffer, block on `$EDITOR` (else `$VISUAL`), parse-validate the saved buffer as a whole
ball, and hand it to the SAME update seal — never a new verb, never an unvalidated commit (a buffer
that won't parse re-edits or aborts; an unchanged buffer is a no-op). Mutually exclusive with the
field flags (they would race over one payload) and interactive-only — a non-tty stdin or an unset
editor is an ERROR, not a fallback, so agents stay on the flag-driven path. The id is the path and
`created` is history, so neither is hand-editable; `updated` is restamped by the seal. Both `create`
and `update` honor a **`--` end-of-options separator** (the getopt convention, bl-d31f), so an
untrusted `-`-leading positional cannot hijack a flag: `bl create -- "$TITLE"`. balls stages
the edit; `update.pre` may reject/adjust; seal; `update.post` reactors propagate (the tracker pushes;
an external-mirror plugin reflects the new title). claim / unclaim / close are NAMED
specializations of `update` (they fix specific fields and, for close, stage a deletion) — kept
as distinct ops because the op NAME is the §6 hook-dispatch key, so a plugin wires into `claim.post`
rather than sniffing "an update that set `claimant`."

**`close`** (retire a claimed ball) — **deliver + retire across the seal boundary; this is what `review`
used to gesture at.** A task's deliverable life is claim → write code → DELIVER → RETIRE; in the
self-merge default DELIVER and RETIRE are one act:
- `close.pre`: (1) the delivery plugin DELIVERS — direct: folds integration into `work/<id>`, runs the
  project repo's pre-commit hook on the merged tree (the delivery gate, §11 — a failing gate aborts the
  close here, pre-seal), then squashes `work/<id>` → integration (conflicts
  surface HERE), sorting LAST so its un-squash rollback is rare; forge: idempotently pushes `work/<id>`
  + opens/updates the PR (rollback is a no-op — a pushed branch + open PR is the correct in-review
  state, §11). (2) core rejects if any close-blocker is open — `closeable()` false (§10) — for forge that is the claim-time approval gate child, so an unmerged PR simply leaves close blocked. The check is abort-safe: evaluated before delivery and the seal, a blocked or failed close aborts BEFORE the ball-deletion seals, so the task stays alive.
- balls seals the `tasks/<id>.md` DELETION (`bl-op: close`).
- `close.post`: the delivery plugin tears down the code worktree (§11); the tracker pushes the
  balls-state deletion commit (NEVER the project code branch).

Core close ships no code, pushes no project remote, runs no source-state check. Forge review is **not
a mode** — it is the ordinary close plus an approval gate child the forge plugin adds at claim
(§10/§11), enforced by core's close-blocker guard (§10). "Close is related to remotes" = the tracker
pushes the *balls* branch, nothing more.

**There is no `drop` verb** (deleted 2026-06-09, bl-65e0 — §15). Closing is the ONLY retirement, so
a `--blocks close` gate guards every way a ball can die. Abandonment is the composite spelled out:
`unclaim` (clears occupancy, releases the worktree — uncommitted work dies with it) then `close` (the
empty deliverable makes §11's delivery a no-op). A legacy `bl-op: drop` deletion in store history
stays valid bedrock — it reads as closed, like every retirement (§3/§5).

## §10 Blocker & gating model

The one relational primitive is the **blocker** (§3): a `{id, on}` edge on the BLOCKED task naming
which of its own ops is gated. `on` is ANY op; `claim` and `close` are simply the two cases with
create-time sugar (front-door flags below). **Blocking is fully separate from containment** — a
`parent` pointer is display only and gates NOTHING. An edge gates a transition ONLY because you
created that edge; nothing is implied. This is the load-bearing subtraction: the old `--parent`
silently minted a claim-blocker, conflating "lives under" with "gates," which is what produced the
late-add gaps. Now every edge is explicit and says exactly what it gates.

- **dependency** = a claim-blocker ("can't START until it resolves").
- **gate** = a close-blocker ("can't FINISH until it resolves"). Build/test/approval/forge/review are
  all this — a "review gate" is not a mechanism, it is `--blocks close` plus a skill-doc convention.
- **epic** = a task with children — a pure CONTAINMENT/display rollup, emergent from `parent`
  pointers, gating NOTHING by itself. If you want "the epic can't START until its children land,"
  add a `claim` edge per child (`--blocks claim`); if "can't FINISH until they land," a `close` edge
  (`--blocks close`); if neither, the epic is freely claimable and closeable alongside open children
  (their `parent:` simply dangles — display-only, §3 — never corruption). The presumptive pattern is
  a skill-doc hint, not a core rule.

**ready(A)** = A is live (file exists — not closed) + unclaimed + every CLAIM-blocker
resolved. ready(A) true is exactly the **ready** display state (§3); claimed and blocked are the
other two. `bl list` orders the ready set — and every listing — by `priority` ascending (lower =
higher; absent last), then `created` ascending; ordering is display-only, never part of the predicate
(§3). The old `bl ready` is now `bl list --status ready` (§9). (A gate child is a live child that does NOT affect readiness — it's a close-blocker, so it
never shows as a status either.) **closeable(A)** = every CLOSE-blocker resolved; checked by core at close. "Resolved" = the blocker's file is gone.

**Deadlock avoidance is now structural-by-default.** Because no edge is ever auto-minted (containment
implies none), the reciprocal edge that would deadlock is simply never created unless you spell out
both halves yourself. The standard gate is ONE edge — a gate child close-blocks its parent; the parent
does NOT block the gate child, so the gate is freely claimable. The gate-check runs against the
parent's pre-delivery WORK BRANCH (which exists once the parent is claimed), so nothing needs the
parent formally "done" — no `review` window is resurrected. "Do the gate after the parent's work" is a
skill habit, deliberately UNENFORCED. A cycle is now only reachable by explicitly authoring both
edges; it is still not gated by code (links are mutable — unlink to fix). Readiness is immediate-only
(a blocker resolves when its file is gone, not transitively), so a cycle never drives a recursive
walk: it simply manifests as mutual permanent-block, and `claim` refuses naming the unresolved
blocker, which `bl update` then unlinks.

**Enforcement is CORE.** Core stores the schema (`parent`, `blockers`, `tags`) AND enforces every
blocker (the `.pre` of the named `on` op rejects while unresolved), joining the one occupancy guard
(`claim` refuses an already-claimed ball) as core's small fixed set of hardcoded rejections. `claim`
and `close` are the two with their own predicates:
- `claim` refuses an unresolved claim-blocker (`ready()` false).
- `close` refuses an unresolved close-blocker (`closeable()` false).
Both read `blockers` against file-existence ("resolved" = closed = file gone) — the same
`ready()`/`closeable()` predicates core already computes, so enforcement costs a rejection, not a
mechanism.

*Why core, not a plugin (revises the original "gating plugin is the sole enforcer" — bl-70f5):*
balls is unopinionated about workflow, but a primitive must mean what it says: a blocker that does
not block is not a feature. And enforcement must not be severable — forge and build gates create
gate *children* and rely on those children *blocking*, so a separate gating plugin would make
forge/build silently depend on it being installed: an implicit, unverifiable plugin-on-plugin
dependency, and plugins coordinate only through core schema, never by depending on each other's
presence (§0). Gate *resolution* stays fully pluggable (who creates and closes a gate child —
forge/build/human); only the *blocking* is core. A team wanting no enforcement simply creates no
blockers; there is no enforcement to uninstall.

**Front-door flags are CORE** (they write core schema; plugins are hook binaries and do not extend
`bl`'s parser, so there is no plugin-injected-flag composition). Two ORTHOGONAL flags, one for each
fact — containment and blocking never travel together implicitly:
- `--parent E`  → child `parent: E`                          (CONTAINMENT only — gates nothing)
- `--blocks OP` → `E.blockers += {child, OP}` where `E` is the `--parent` (or `--blocks ID:OP` to gate
  an explicit non-parent `ID`) — the new task gates that task's `OP`. The two everyday spellings are
  `--blocks claim` (subtask must finish before the parent can START) and `--blocks close` (gate: must
  finish before the parent can CLOSE). `OP` is required — there is no default-gated transition.
- `--needs B` (or `--needs B:OP`) → `self.blockers += {B, OP}`, default `OP = claim` — the inverse
  direction: the new task is gated BY `B` (cross-tree dependency).

The retired `--gates X` is exactly `--parent X --blocks close`. Any edge the one-liner can't express
(gate a third op, multiple blockers, a post-hoc edge) is an ordinary `bl update … blockers` edit —
the create flags are sugar over the general primitive, never a constraint on it.

**Gates are tasks only** — every gate-check is "is task X closed?". Build/test = a build-gate child a
build plugin creates and closes on pass; forge/PR approval = a child the forge plugin creates and a
forge `sync` closes on merge; human approval = a child a person closes. Creation rides `claim.post`
(the deliverable signal) so non-deliverables stay clutter-free. The resolution mechanism is pluggable;
the blocking mechanism is one thing.

(The old "late-added subtask doesn't gate a claimed epic's close" gap is DISSOLVED, not patched: it
existed only because `--parent` auto-minted a *claim* edge. With containment and blocking separated,
you state the gate you want — `--blocks close` gates close whenever the child is added, claimed epic
or not — and if you state nothing, nothing gates, by design. No skill-hint "don't" required.)

## §11 Delivery / worktree plugin

A SIBLING of tracker (default-wired, separate — so worktrees-without-remote ⊥ remote-without-
worktrees). It owns the deliverable CODE worktree — a `git worktree` of the PROJECT repo on branch
`work/<id>` — end to end. Base balls never opens the project repo, so "nothing on main / nothing in
the project tree" is structural.

**Kind-blind.** The plugin NEVER branches on task kind. It materializes a worktree on `claim.post`,
delivers-if-changes + tears down on `close`, blind to whether the task is an epic, a gate, or real
work. Non-deliverables are normally closed without being claimed → no worktree is ever made; a
claimed non-deliverable gets a harmless EMPTY worktree (the close.pre squash is a no-op when
`work/<id>` has no changes; close.post remove is a no-op when the path is absent). This dissolves any cross-plugin tag coordination — the worktree plugin reads no blocker state at all.

**Derived path; stateless across ops.** The worktree path and branch are pure functions of
`(binding, id)`:
```
worktree_path(binding, id [, claimant]) = $XDG_STATE_HOME/balls/plugins/<name>/<binding.invocation_path>/<key>/
branch                                   = work/<key>          # <key> = <id>, or <id>-<claimant>
# invocation_path is MIRRORED verbatim (leading / stripped), not percent-encoded: this is a cargo
# build dir and rust-lld cannot open an output file under a `%` ancestor (bl-f3e4, see §1).
```
`<id>` (and `claimant`, if keyed on) ride the post wire — `<id>` is the immutable `bl-id` trailer,
`claimant` an ordinary frontmatter field — so the plugin RECOMPUTES its resource each op and checks
the filesystem; it needs no id-keyed scratch, and every hook is idempotent by construction. Keying on
`claimant` as well as `id` keeps the pure-function/stateless property (claimant is already on the
wire) while disambiguating an unclaim-and-reclaim by a different actor and naming forge branches by owner.

**Core never computes this path, and nothing stores it.** The path has an authoritative home already:
it is a pure function of `(binding, id, claimant)` AND a standing local git fact — `git worktree list`
in the project repo prints `work/<id> → <path>`. The formula is how the plugin PLACES the worktree;
git's worktree registry is the ground truth of where it landed. Recording it in a frontmatter field
would be a THIRD copy of a fact git already owns — and a derive-don't-store violation (§0): the path is
machine-LOCAL (`$XDG_STATE_HOME` + `invocation_path`), so the default delivery+tracker combo would
seal-and-PUSH a local path into the shared store, replicating it to every clone where it is meaningless.
The claim itself is shared truth and belongs on the remote; its worktree is a local derivation and does
not. So the plugin stores nothing and surfaces the path ON DEMAND, two ways — both PRINTING, neither
teaching core the formula:
- on `claim.post` and `prime.post` it PRINTS the path on stdout (§6), which balls forwards verbatim —
  the natural product of the verb (as `create` prints the id) at the two moments a consumer transitions
  into a worktree: starting work, and resuming a session (`prime` lists the claimed set and prints each
  path). This is deterministic, not interleaved noise: stdout is reserved for the verb's ONE product —
  tracker and every other plugin write diagnostics to stderr (§6), so delivery is the sole stdout writer
  on claim/prime. An agent reads it as readily as a machine parses it; the bl-934a worry that stdout is
  "not machine-parseable" held only if plugins muddy stdout, and the discipline forbids that.
- on `bl show` it answers a READ-OP dispatch (§6): core, which never opens the project repo, asks the
  delivery plugin for the worktree and folds the printed line into the HUMAN render. `bl show --json`
  does NOT dispatch — it is the lossless mirror of stored frontmatter (§9), and the worktree is not
  stored, so the machine contract never carries a local path. A machine that wants it scripts `bl claim`/
  `bl prime` stdout, or `git worktree list` directly.

The plugin computes the path from `(binding, id, claimant)` (or reads `git worktree list`) every time
it needs it — materialize, release, rollback, and each surfacing — so it holds no id-keyed scratch and
every hook stays idempotent (the pure-function/stateless property above). The worktree lives in plugin
XDG territory, not the project tree; its existence is git's to know, never a ball field's to assert, so
there is no field that can drift out of sync with whether the worktree exists.

**Hooks:** `claim.post` materialize (create-if-absent) + print the path; `prime.post` re-materialize
each still-claimed worktree + print its path; `unclaim.post` releases (remove-if-present);
**`close.pre` deliver** (sorts last); **`close.post` teardown**; **`show` (read-op)** print the path for
the named ball (§6 read dispatch). balls does not guard against tearing a
worktree down from inside it — the agent SHOULD `cd` out of the worktree before closing so its shell
cwd is not deleted underneath it (a recommendation in the skill guide, not an enforced precondition).

**Two variants** (only "what's wired into the delivery hooks" differs; both kind-blind and idempotent):
- DIRECT (default, local-squash): `close.pre` delivers in four acts, the first three in the code
  worktree (re-materialized if absent): CAPTURE any pending work onto `work/<id>` (`--no-verify` —
  the gate runs once, below, not per-capture); FOLD integration into the branch (a real merge, so
  the tree gated next IS the tree that lands even when integration moved since claim; a conflict
  aborts the half-merge and the close); GATE — run the project repo's own **pre-commit hook**
  (bl-ee85: the squash is plumbing, so without this a close bypasses the hook every porcelain
  commit runs; resolved as git resolves it — `core.hooksPath` honored — and skipped as git skips
  it — absent or non-executable hook = ungated project, delivers as before; a failure aborts the
  close BEFORE the seal, leaving claim and worktree up for the fix); then SQUASH `work/<id>` →
  integration as one commit
  whose subject carries the `[bl-id]` delivery tag (the plugin's analog of the §5 trailer; this tag
  is delivery ground truth). Integration branch is the delivery plugin's own config (default
  `HEAD@project-repo`); a per-task override, if ever needed, rides as a preserved frontmatter key
  (§3 seam), NEVER a core field — core opens no project repo, so it has no integration branch to name. No gate child.
- FORGE (opt-in): the forge plugin adds an **approval gate child** at `claim.post` (a normal
  close-blocker, §10 — NOT a special mechanism; identical to a build or audit gate), and at `close.pre`
  idempotently pushes `work/<id>` + opens/updates the PR. The forge produces the squash (the merge), so
  `close.pre` does NOT squash locally. A forge `sync` closes the gate child on merge → the next close
  unblocks. Core enforces the block (the close-blocker guard, §10), not a bundle-private check. An empty deliverable's gate is auto-resolved (the plugin closes its own gate
  child at `close.pre` when `work/<id>` has no changes — nothing to review), preserving kind-blindness.

**`delivered_in` is a derived query, not a field** — "delivery IS the tag, not a field." The plugin
answers "where was `<id>` delivered?" by tag-scanning (`git log --grep [bl-id]`) the integration
branch; no stored hint, no write/null asymmetry, no staleness. **Id-reuse stays unambiguous by
RECENCY** (the subtlety bl-d7a5 deferred here): a 4-hex id may be reused across incarnations, so
several `[bl-id]` commits can accumulate on the integration branch — but a reused id only begins after
the prior incarnation CLOSED, so its delivery necessarily lands LATER; deliveries are monotonic with
incarnations. `bl show <id>` reconstructs the k-th-most-recent incarnation from `balls/tasks` history
(§9), and its delivery is the k-th-most-recent `[bl-id]` commit on the integration branch — the SAME
live-first-else-most-recent walk §9 applies to the ball file. So the cross-incarnation grep ambiguity
is dissolved WITHOUT a field. Storing `delivered_in` was infeasible anyway: `close` seals the ball-file
DELETION as one commit (§9), so a `close.pre` frontmatter write never lands in any recoverable
file-present tree, and the recency walk reads the PRE-close commit, which predates delivery — the
write would simply be eaten by the deletion. Derived + recency-ordered keeps the no-field / no-staleness
property AND the disambiguation. (A cross-clone miss is reported honestly or resolved by a tracker fetch.)

**Rollback** (specifics; general rule §14): project-repo commits are tier-2 (`git reset`, not covered
by the change-worktree/store un-seal). `rollback claim.post` = remove the worktree + delete
`work/<id>` (forge: also drop the just-created gate child); `rollback close.pre` direct = `git reset
--hard HEAD~1` un-squash (reversible — nothing pushed); forge = **no-op** — a pushed branch + open PR
is the correct in-review state, never undone (abandon is `bl unclaim`, whose `unclaim.post` tears
down the PR/branch). `close.post` teardown removes the worktree DIRECTORY (re-creatable from the branch, so it
is rollback-safe); deleting `work/<id>` is deferred, non-transactional cleanup (`prime`).
The only irreversible action in a close is therefore the tracker's final push, which sorts LAST.
And because a rollback is best-effort (§14), deliver itself is RETRY-IDEMPOTENT (bl-430e): a squash
that survived an aborted close — `work/<id>` fully merged into the integration branch, or a `[bl-id]`
commit on it since the branch forked (fork-scoped so a reused id's PRIOR delivery, always an ancestor
of the fork, cannot false-positive) — means this incarnation already delivered, and the re-close
skips the squash instead of minting an empty duplicate delivery commit; retrying the failed `bl
close` is the sanctioned recovery.

## §12 bl prime & federation (the store pointer)

`prime` is the **idempotent** verb that brings a checkout into readiness — "make me ready to start
the engine." It absorbs the old `init`: founding is not a separate verb, it is `prime`'s
**bootstrap-on-miss** path. Run it on a fresh dir and it founds; run it on an established checkout and
every step is create-if-absent → no-op-converge. **Converging predicate:** the landing
(`balls/config`) exists and the store (`tasks_branch`) resolves to a valid, current `tasks/` checkout,
whether the path was empty, fresh-cloned, freshly-onboarded, or already established — base balls
cannot tell which ran, so re-running prime is never an error (no `--reinit`).

Core only (a) ensures the landing + store substrate and (b) runs the configured plugin chain, then
commits — it has zero knowledge of tracker/remotes/stealth. **The local-miss branch SEEDS a fresh
landing by copying the app-level `default-config/` folder (§1) into `balls/config`** (`git init` if no
repo; one commit). The copy is not byte-blind: `balls.toml` travels verbatim, but the seeded
`plugins.toml` is written with each named plugin BOUND to its sibling binary beside `bl` and every
entry whose binary is absent PRUNED (`src/seed.rs`) — so a tracker-less or test box seeds a schedule
it can actually run instead of aborting on first dispatch. The prune is seed-time only: §6's dangling
`bin/<name>` "referenced but not installed here" clean error still governs an ESTABLISHED landing
whose schedule names an uninstalled plugin (e.g. wiring that arrived by `install`); re-prime never
prunes a committed schedule, it only re-derives the gitignored `bin/` symlinks. The seed is where the
tracker + delivery + builtin plugin wiring comes from — so
`prime`'s `prime/pre`/`prime/post` run the plugins NOW IN THE LANDING LIST (there are no run-time
defaults, §0/§4); on an established landing the seed step is a no-op (config already present). **The
store is NOT founded with the landing — it is materialized LAZILY inside `prime`'s fixpoint (bl-0a23,
§2/§8):** core loops `prime/pre` (where the tracker clones an established remote branch into a local
ref) then `materialize(tasks_branch)` (check the cloned-in ref out, or found a fresh orphan iff no ref
exists) until the configured name stops moving, THEN runs `prime/post`. Founding the orphan eagerly,
before the tracker could clone the remote in, was the unrelated-histories divergence bl-fa00 had to
reset away; lazy materialize means it is never created. Per-session worktree re-materialization for
still-claimed tasks rides the same chain (the delivery plugin's `prime.post`, idempotent
create-if-absent, §11).

**Tracker's prime — two slots, one per axis (bl-0a23).** With no remote it is STEALTH (store stays
local, a self-lock written). With a remote (`--remote` > `--center` > XDG > `origin`):
- **`prime/pre` settles the NAME and clones the store in.** It WARNS when the store sits elsewhere (a
  default-named clone of a repo whose canonical store is a non-default branch — diagnostic only; it
  NEVER rewrites `tasks_branch`, because config crosses into a landing solely by `install`, §0/§12).
  Then it CLONES IN: when the remote already carries the store branch and this clone has no local ref
  by that name, fetch it straight into `refs/heads/<branch>` so core's `materialize` checks it out —
  an established history adopts with no divergent orphan to reset. A local branch already present is
  left for `sync` to fast-forward; an absent remote branch is the bootstrap, left for core to found.
- **`prime/post` settles the CONTENT.** Established remote branch → fetch-ff (bring current) then push
  (publish). Absent branch → the founding push CREATES it.

Remote founding is therefore **gated by having the tracker at all**: the opt-out is structural — drop
the tracker or `--stealth` and prime never touches a remote (the seeded tracker *is* the consent to
leave a branch on `origin`). Adopting an established store branch and founding an absent one are the
same read-from-remote step, the difference read from remote state, not a flag. **Implicit founding is
fine:** creating a `balls` branch on a repo you can push to is harmless and once-per-clone — `--stealth`
opts out (and locks the store local).

**Push-failure splits on founding-vs-established — the two must NOT collapse to one silent path
(bl-9857).** `prime/post` reads the case from remote state, the same `ls-remote` that decides
adopt-vs-found:
- **Founding-miss** (branch ABSENT, no create perm): the bootstrap push that would CREATE the branch is
  rejected. This falls back to stealth-local **silently** — nothing existed to land on, so "couldn't
  found, stayed local" is harmless by definition and once-per-clone, and the property holds even without
  write access. (Re-running `prime` re-attempts; if another clone has since founded the branch it is now
  present → adopt.)
- **Rejected push to an ESTABLISHED remote** (branch PRESENT — non-ff, perms revoked mid-life, a
  server-hook reject): this is the opposite and is an **ERROR (E5)**. Your mutation did NOT land while
  you believe you are federated; silently degrading to stealth here is a split-brain (the local store
  diverges from the remote everyone else reads). The non-zero exit aborts the op (the §13 pull → mutate
  → push contract), surfaced, never swallowed. `prime/post`'s OWN established publish (fetch-ff + push,
  bl-0a23) takes this same E5 path — it is exactly every op's `*/post` publish (the tracker's
  `remote_ops::push`); only the founding push, where nothing existed to land on, degrades silently.

**Federation = many landings, ONE store branch.** There is no trail, no terminus, no transitive
discovery, no `operating/` symlink (all retired with config-shadowing — §4). A center is not a special
branch: it is just whatever `tasks_branch` a set of checkouts have agreed to share, backed by a common
remote. Federating is two edits, both consented:
- **`tasks_branch` names the store** (a §4 config field on the landing). Point it at a shared,
  remote-backed branch and your checkout reads/writes that store; point it local and you are stealth.
  Changing it is an ordinary config edit — and config changes only by you or by `install` (§6), never
  silently.
- **`bl install --from <center>` adopts the center's config + plugins**, with consent — how a checkout
  learns a non-default `tasks_branch` and gains the team's tooling. **The STANDARD case needs no
  install:** the seeded defaults name `origin` + `balls/tasks`, so a fresh clone of a repo whose store
  sits there runs `bl prime; bl list` out of the box — prime adopts the existing store (a read), no
  config adopted. **Anything non-standard needs install** — a center's config, a non-default branch, a
  third-party plugin — because adopting config is potential RCE and crosses only by the explicit
  `install` (§0/§4). `prime` is auto-safe (it seeds the LOCAL default and syncs what landing already
  names); it CANNOT adopt a foreign config or activate third-party code — that power lives only in the
  deliberate `install` (and `prime --install`, §13).
- **The implicit `origin` is the PROJECT repo's, discovered by the tracker.** The bottom precedence
  tier (`origin`) means `git remote get-url origin` on the **invocation path** — the project repo the
  user cloned, whose `origin` is the real remote the code rides and where `balls/tasks` sits alongside
  it. It is NEVER read off the landing: the landing is local-only (§2 install-transport), founded by a
  bare `git init`, and carries no origin — reading origin there is meaningless. And like all
  remote-talk it is the **tracker's** job, not core's (§0): core resolves only the EXPLICIT tiers
  (`--remote`/`--center`/XDG `remote` — config reads) and hands the tracker `remote: None` when none is
  set; the tracker then discovers `origin` from the invocation path as its single fallback, in ONE
  place all its handlers share (not re-probed per op). This is what makes "a fresh clone, `bl prime`,
  works out of the box" true without a flag.

**Non-default store, no install → a WARNING, not silence.** The one ergonomic gap: clone a repo whose
store is on a NON-default branch and `bl prime; bl list` shows nothing (the seeded default points
elsewhere). prime fetches the standard `origin:balls/config` regardless (reading is free), so the
tracker has it in hand — and if the landing's `tasks_branch` is still the SEEDED DEFAULT (not user-set,
not `--stealth`) AND the synced `origin:balls/config` names a different store, the tracker EMITS A
WARNING ("this repo's tasks are on `<branch>` — run `bl install` / `bl prime --install`"). Diagnostic,
never authority — it changes nothing and executes nothing (§0). A non-standard branch with no
`origin:balls/config` to read is uncatchable — so be it.

**"Center" is emergent.** No branch declares itself a center; a center is just a store branch that ≥1
landings' `tasks_branch` values converge on (in-degree). Setting up a center = point your
`tasks_branch` at the shared branch and `install` it to the checkouts that should join. Founding
(remote store branch absent → create+push) vs joining (present → adopt) is read from remote state,
never declared by a flag.

**Re-homing — stealth ↔ federated (bl-0601, revised).** There is **no `adopt`/`disown`/`remaster`
verb**. "Stealth vs federated" is just whether `tasks_branch` is local or remote-backed, and re-homing
is the two directions of one invariant — *the store moves to its new home BEFORE its name changes* —
decomposed onto two scopes of two verbs that already own the halves:
- **The name is config's.** Repoint `tasks_branch` (a §4 config edit on the landing); the tracker
  syncs the new branch on the next `prime`/`sync` (established-vs-fresh stays the read-from-remote
  sync-or-bootstrap fork above, never a flag).
- **The store is `install`'s.** Merge the store into its new home with `bl install tasks/* --from <old>
  --to <new>` (glob = union, §6) when the home is populated, or `bl install tasks` (folder = mirror)
  into an empty home. `tasks/` is a committed subtree, so this is just install's path-copy. Disown
  (federated→stealth) is the reverse: pull the store down to a local branch, then repoint `tasks_branch`
  local.
Order is non-destructive by construction: the store moves to its new home BEFORE the pointer changes;
the glob union is additive (a same-`id` overwrites, git-recoverable, §6). Config and store are just
different *paths* of one path-copy verb. No new verb, no new dispatcher surface, no `bl tracker`.

**Config never auto-layers (bl-62bc revised — see §4).** The old "config VALUES layer automatically
down the trail" is RETIRED: there is no trail, and a center's config reaches you only by explicit
`bl install` (§6), materialized into your landing once, with consent. Executable plugins were already
install-only/consented; now config VALUES are too — one uniform rule, *nothing crosses a checkout
boundary except by `install` (config) or `sync` (the store)*, replacing the old asymmetry where
values silently shadowed but executables didn't. A center is the source of truth for the RECOMMENDED
config + plugin set; the committed `plugins.toml` schedule travels on install, the local `bin/<name>`
never does, so a center can never make your box run a binary you didn't opt into.

**Sync is two-tier (bl-62bc revised; verb mechanics → §13).**
- **The store** (`tasks_branch`) syncs **every op, default ON** — you push mutations (claim/close) to
  the remote store branch, so each op runs against an up-to-date store (pull → mutate → push). A
  tracked store that isn't current is a surprise; an offline knob exists but the default is synced.
- **Config never syncs.** It is landing-local and changes only by `install` (consent, §6). `sync`
  moves the store ONLY — it structurally cannot re-home your config or activate code. There is no
  topology to refresh beyond the `tasks_branch` value itself, which is config.
- This generalizes: per-op sync is just what a stateful plugin's `sync` does (tracker syncs the store;
  a jira plugin syncs issue state). Not tracker-special — the synchronization contract.

SEAM: core reads config from the landing and reads/writes tasks on `tasks_branch` (knows nothing of
remotes — stealth = a local `tasks_branch`, identical code); the tracker translates remote↔local for
the STORE branch only (fetches the configured remote into the §1 layout, keeps the store synced). The
physical realization is §1 (`config/` and `tasks/` are two checkouts; the store checkout tracks
`plugins/tracker/<remote>/` when remote-backed, a local branch in stealth — no `operating/` symlink);
`bl sync`/`prime` verb mechanics + contention are §13.

Error/notice catalog (verbatim, ownership in brackets): E1 [tracker] no store remote resolved
(stealth/no-tracker is fine — this fires only when a remote was named but unresolvable); E4 [tracker]
remote unreachable (refusing to bootstrap); E5 [tracker] push rejected by an ESTABLISHED remote store
(non-ff / perms revoked / server-hook reject — the mutation did not land; the op aborts per the §13
pull→mutate→push contract, NEVER a silent stealth degrade — bl-9857); E7 [balls] plugin failed during
prime, rolled back K prior; W1 [tracker] store is stealth (local), not auto-syncing. (Retired by idempotent prime: E2
"already initialized" — re-running prime is a no-op-converge; E3 "remote already established" —
established vs absent is the adopt-vs-bootstrap fork, not an error. Retired by the trail's removal: N3
downstream-layer-introduces-plugins — there is no downstream layer.)

## § id generation

The id scheme is FIXED — `bl-` + 4 lowercase-hex digits (`{ prefix, length, alphabet }` =
`{ "bl-", 4, "0123456789abcdef" }`), no generator enum and NOT a config knob. Base balls ships ONE
generator (random) over that one scheme; ANY customization — a different prefix/length/alphabet, OR a
non-random strategy (timestamp/sequential/uuid) — is a `create/pre` plugin via the same `git mv`
reassign seam, so "custom generation" and "plugin-assigned id" are one seam. **Validation is
string-safety**, not an arbitrary charset: `^[A-Za-z0-9][A-Za-z0-9_-]*$` (no `/`, no `.`, no
whitespace/metacharacters, no leading `-`). The default alphabet is lowercase to sidestep
case-insensitive-FS collisions. **Collision** is checked against LIVE files only: auto-gen → retry
(bounded); plugin-assigned → abort (an explicit choice is authoritative). A regenerated id that matches
a DEAD (closed) incarnation is LEGAL — no history scan to prevent reuse. id is IMMUTABLE after
create; reassignment is a create-only capability, so during claim/close it rides the wire with no skew.

**Recency resolution (the unifying discipline).** Every id→content lookup is the same walk: live
`tasks/<id>.md` first, else the most-recent-in-history incarnation, stop at the first hit (§2, §9 —
`show` fallthrough, `list -s closed/--all`). This DISSOLVES the id-reuse-vs-history concern: at most one
incarnation is ever live (the filesystem guarantees it), so a reused id unambiguously means "the
current incarnation, or — if none is live — the most recent dead one." The which-version problem is
solved by construction; the 4-hex space stays fine, no widened ids, no extra collision check.
Prevent-reuse was the wrong lever; recency-order resolution is the right one.

## §13 bl sync & bl prime

`sync` and `prime` are ordinary ops (§8) — each with `sync.pre|post` and `prime.pre|post` hook keys,
every listed plugin invoked in list order, failure → reverse-order rollback. They author NO
ball-file diff, so there is **no change worktree** — plugins run with cwd = the store checkout
(`tasks/`) and act on the filesystem there (§7), never through a return channel. (The one exception is
`prime/pre`, which runs with cwd = the LANDING: on a first prime the store is not materialized yet, so
it cannot be the cwd — the tracker's clone-in fetches into the landing, and the materialized store
is the cwd for `prime/post`, bl-0a23.) Core commits nothing of its own; the only state that moves is
remote-authored commits a plugin imports, or external/derived caches it refreshes.

**`bl sync` — the synchronization primitive.** Low-level, run often: it makes state consistent and is
mostly a verb for plugins to hook. `bl sync` (no arg) syncs the **store** — fast-forwards/unions
`tasks_branch` with its configured remote; `bl sync <branch>` syncs a named branch. It moves **task
state only**: config is landing-local and changes only by `install` (§6), so sync *structurally
cannot* re-home your config or activate code — it touches the store branch and nothing else. The store
branch may be a shared one the whole team names (`tasks_branch`); syncing it is ordinary federation,
not a consent breach, because consent governs config + executable plugins, never store currency.

- **`--branch` names a sync TARGET, and the landing is never one.** `bl sync --branch landing` is a
  no-op — *for free*, not by special-case. The landing is the local, path-derived config branch (§2):
  it is single-owner, has no upstream to fetch, and holds no task state to move (tasks live on
  `tasks_branch`). The general rule "fetch a branch's upstream, if any" yields nothing on an
  upstream-less branch, and the landing is upstream-less by construction (§4 — it is never a sync
  target). The landing changes only by `install`, never by sync.
- **No separate contention probe.** The tracker's hook is a single `git fetch` + **fast-forward-only**
  integration; that one operation is atomically detect-and-act — a non-ff IS the contention signal,
  surfaced as the tracker's non-zero exit ("remote wins, re-run"). A distinct `sync/pre` "has the
  remote moved?" check is rejected: it adds a round-trip and a TOCTOU window and duplicates what
  ff-only already decides in one step (contention is the ff-failure path, not a phase of its own).
- **Where the integration sits.** With no core commit, sync's pre/post boundary IS the tracker's
  fetch+ff: the tracker is wired into **`sync/pre`** so it imports remote store state first;
  **`sync/post`** plugins (cache rebuild, worktree re-materialize) react to the now-current store. A
  non-ff aborts in pre and never reaches post. This is the one op whose boundary action belongs to a
  plugin, not core — because remote-talk is plugin-exclusive (§0).

**`bl prime` — readiness, built FROM sync.** prime is not a hook-superset of sync; it is an
**orchestrator of syncs** (§12): found the landing, run the `prime` FIXPOINT to materialize the store,
then `bl sync` it. "Ready to start the engine" = substrate exists + store synced + claimed-task
worktrees re-materialized (delivery's `prime.post`, §11). Idempotent: on an established checkout every
step is create-if-absent/already-current, so re-running converges to a no-op. The whole verb is the §12
converging predicate.

- **prime is a bounded fixpoint, core-owned (bl-0a23).** Core loops `prime/pre` then
  `materialize(tasks_branch)` until the configured name stops moving, THEN runs `prime/post` once. The
  loop's signal is the config branch core owns — no §7 return channel, no plugin-driven control flow —
  and it is bounded by the §6 invocation-tree cap. `prime/pre` (the tracker's name-settle + clone-in)
  runs against the landing; `post` (fetch-ff + publish) against the now-materialized store. It converges
  in ONE `pre` pass in the common case (the dial holds), since plain prime never rewrites `tasks_branch`
  — that crosses only by `install` (§12). The fixpoint is what lets an established remote branch be
  cloned in and checked out before any orphan is founded, so the bl-fa00 unrelated-histories divergence
  is never created.

- **prime drives sync; it does not duplicate it.** Keeping the fetch inside the sync primitive is what
  makes "all wire transfer goes through sync" a true single-codepath invariant. prime's only distinct
  work is substrate + consent-free local readiness + materialization; currency it gets by *invoking
  sync*, not by reimplementing a fetch. On an already-joined checkout prime is exactly the sync you'd
  run by hand plus the idempotent worktree pass — nothing more.
- **prime ≠ install, but `prime --install` fuses them on demand.** Plain `prime` is auto-safe and runs
  every session: it seeds the local default (§12), syncs what landing already names, and NEVER adopts
  foreign config or activates third-party code. Adopting is the deliberate `install` (§6).
  **`prime --install <center>`** is the one-command first-contact / re-adopt: prime ensures the local
  substrate, `install` copies the center's config + plugin wiring into the landing (§6, consent-gated),
  and a final prime brings the just-adopted `tasks_branch` to readiness. The center's config gets here
  the way all remote bytes do — the TRACKER, never core, does the fetch, wired on the default
  schedule's `install.pre` hook (§6 table): it fetches the center's `balls/config` into the landing's
  `FETCH_HEAD` (a git-standard ref, no invented core↔plugin convention), where the install path-copy
  reads it. A READ only — never a push to the center (publishing is `install --to`); stealth (no
  remote) is a no-op. Remote-talk stays plugin-exclusive (§0) even inside the fused verb. It is a
  SINGLE hop, not a walk: a center's config is self-contained — it names its own `tasks_branch` (the one config→store
  indirection, §4), never another config to chase — so there is no chain to recurse. (The older
  recursive multi-hop form was a config-*trail* artifact, retired with config-shadowing: §4/§12 —
  config crosses a checkout boundary exactly ONCE, by explicit install.) Each install is idempotent,
  so a failed adopt resumes rather than double-applies. The `--install` flag IS the consent — plain
  prime still cannot activate code, so the auto-safe property holds for the every-session path.
  Likewise prime never *pushes* config; outbound transfer is `install --to` (§6).

**The wire (§6/§7), minus the task-shaped fields.** A sync/prime plugin gets meaning from its
`binding`, not from a command:

- **argv:** `<op> <phase>` (`sync|prime` / `pre|post`). **env:** `BALLS_PROTOCOL=1`,
  `BALLS_PLUGIN_NAME`, `BALLS_PLUGIN_DEPTH` (the §6 set). **cwd:** the store checkout (`tasks/`).
- **pre stdin:** `{ protocol, op, phase, plugin_name, actor, binding }`. `binding =
  { remote, tasks_branch, store, landing, invocation_path }` is the load-bearing payload — exactly
  what a fetcher needs (`remote` + `tasks_branch` + the `store` checkout path). **Absent:** `command`
  (nothing is authored — no `body_change`) and `current_state`/`previous_state`
  (sync/prime are not per-task transitions).
- **post stdin:** the same plus `previous_commit`/`commit` — the store branch HEAD before and
  after the op, bracketing what was integrated. `previous_commit` is the one datum a plugin cannot
  recover afterward (the old ref is gone once ff'd), so it rides the wire; `null` on a fresh-clone
  prime where nothing pre-existed. **Still absent:** `metadata` — sync/prime write no balls commit,
  so there is no §5 trailer block to parse. A plugin curious about an arrived commit's trailers reads
  them from the store via git itself (the §7 filesystem-not-return-channel rule); the "trailer
  metadata of the triggering op" framing does not apply — these are top-level ops, not triggered by a
  trailer-bearing op.

**Rollback.** The same reverse-order discipline as any op, but with no change worktree to discard
rollback reduces to each plugin's own `rollback` hook. Most sync/prime plugins are idempotent
refreshers (rollback is a no-op); the tracker's ff is a local ref move with nothing pushed, so a
partial sync leaves the store at either the old or the new HEAD, never wedged — re-running sync
converges (§14).

## §14 Rollback — the general rule

Rollback is the unwind half of §8: any non-zero `pre`/`post` exit aborts the op, calls `rollback` on
each already-run plugin in REVERSE execution order, then core UN-SEALS its own change. One rule
governs everything: **every run plugin's rollback is invoked in reverse; the plugin decides what
"undo" means; core un-seals.** §11 is the worked DERIVED-state example, §13 the no-change-worktree
(sync/prime) case.

**Three side-effect tiers — WHERE it landed decides WHO unwinds it.**
1. **The ball record** — the staged file edits AND core's seal (commit + integrate to the target
   branch: the store for task ops, the landing for config ops). Core un-seals: a PRE-phase abort
   discards the un-sealed change worktree (nothing reached the branch); a POST-phase abort `git reset`s
   the target branch back one commit (local and reversible — core never pushes, so there is nothing
   remote to chase). **No plugin rollback needed for tier 1.**
2. **Commits/refs on ANOTHER local git repo** (the delivery plugin's squash onto the project
   integration branch, its `work/<id>` branch). The un-seal never touches a second repo. **The
   plugin's `rollback` does the `git reset` / branch delete** — locally reversible because nothing
   was pushed.
3. **External side effects** (the tracker pushed the balls branch, jira created a ticket). Not even
   local. **The plugin's `rollback` is best-effort** and may be irreversible.

One plugin can span tiers (delivery: a tier-2 squash plus tier-1 worktree-discardable files).

**Rollback spans the seal boundary — the OP is the unit of atomicity, not the phase.** When any
plugin fails — INCLUDING a `post` reactor after the seal — every plugin that already ran a phase for
THIS op is rolled back, in strict reverse order, regardless of which phase it ran. A plugin wired
into `<op>/pre` but NOT `<op>/post` DOES get a `rollback` call when a later `post` plugin fails: its
pre side-effect is part of the op, and the op didn't happen. The `rolling_back: pre|post` field (§7)
tells the plugin which of ITS OWN phases is unwinding.

**Persistence-through-abort is the plugin's own rollback choosing a no-op — never a core carve-out.**
Core always calls rollback on every prior plugin; whether a side-effect survives the abort is decided
by that plugin's rollback, by the side-effect's semantics:
- the forge plugin's `close.pre` rollback is a **no-op** — a pushed branch + open PR is the correct
  "in review" state, never something to undo (abandon is `bl unclaim`);
- the jira plugin's `create.pre` rollback **deletes** the issue — an orphan ticket for a ball that
  never sealed is wrong.

So an effect persists through a stop IFF the plugin that made it declines to undo it; you can never
accidentally strand another plugin's effect. This dissolves any need for a special "deferred" or
"blocked-close keeps its setup" mode: a blocked forge close is just core rejecting on the claim-time approval gate (`closeable()` false, §10/§11), and the push/PR persist because their rollback no-ops. Two stop
shapes, one rule:
- **blocked** (any gate open: dependency / approval / build / audit) — core rejects before the seal; priors roll back; idempotent delivery effects survive via no-op rollbacks.
- **failed** (jira down, squash conflict, push fails) — priors roll back; each undoes its own (jira
  deletes the issue, the squash `git reset`s).

**Best-effort, no retry, exit code IGNORED.** Core invokes each `rollback` once and CONTINUES
unwinding whatever it exits — it never retries and cannot verify success. If a stubborn plugin's exit
could abort the unwind, one plugin would strand every earlier-run plugin; continue-regardless is the
only composition that fully unwinds. Core's tier-1 un-seal always succeeds (local), so the op's core
invariant holds even if every plugin rollback fails. A FAILING plugin's own rollback is not called —
it cleans up inline before exiting non-zero. Plugins log load-bearing detail to stderr, which balls
envelopes into the unified `clones/<enc>/log` (§6). Rollback MUST be idempotent — safe when the side-effect was never made
or already undone; the derive-and-check pattern below gives this for free.

**post never mutates the ball; derive-don't-store is what makes that safe.** The ball is sealed at the
boundary, so a `post` reactor cannot edit it (that would need a second commit). Anything that must
live on the ball is written in `pre`; a value that only EXISTS post-seal (the integration sha) is
DERIVED on read (§11 `delivered_in` is a `git log --grep` query, never a written-back field), so no
reactor ever needs a return channel. This is why two hooks suffice and post stays purely outward-facing.

**State for rollback: DERIVE first, scratch only when you must.**
- **DERIVE (preferred).** If the resource is a pure function of `(binding, id)` plus the git/
  filesystem state the plugin already owns, store NOTHING: recompute and inspect/undo (remove-if-present;
  `git reset` if HEAD carries the `[bl-id]` tag). §11's delivery plugin — `worktree_path(binding, id)`
  recomputed every op — is the worked example; every hook is idempotent by construction. "Don't store
  what you can compute," applied to rollback.
- **SCRATCH (only for non-derivable state).** A plugin whose intermediate state genuinely cannot be
  recomputed — an id an external service ASSIGNED, a prior value about to be overwritten — persists it
  in its own §1 territory, id-keyed: `$XDG_STATE_HOME/balls/plugins/<name>/<id>/` (prepend
  `pct-enc(invocation_path)` when the resource is per-checkout; §11's delivery worktree keys the same
  way but mirrors that path instead of encoding it, since it is a cargo build dir — see §1). **Env vars cannot serve
  this** — `BALLS_*` is balls→plugin for ONE invocation; it never crosses from a plugin's `pre`
  process to its later `post`/`rollback` process. The filesystem territory does, and it is already the
  plugin's by §1.
- **The id is the one key, and it serves cross-op too.** `bl-id` rides every per-task wire (pre reads
  it from the single staged `tasks/*.md`; post/rollback from the sealed §5 trailer), so ONE convention
  — id-keyed scratch — serves BOTH same-op `pre→post` handoff AND cross-op persistence (`claim.post`
  writes, `close.post`/rollback reads). Scratch lifetime is bounded by the resource: the plugin deletes
  `<name>/<id>/` when the resource is gone (successful terminal op, or after a rollback consumes it) —
  no archive (mirrors §2). Rollback is scoped to ONE op invocation — it never reaches back to undo a
  prior op that already SUCCEEDED, so a rolled-back op leaves prior-op scratch intact for a retry.

**Un-undoable side-effects sort LAST.** A plugin with a tier-3 (or otherwise irreversible) side-effect
is placed LAST in its phase's hook list, so nothing runs after it — making its rollback the RARE
path (rollback fires only when a LATER plugin fails). A CONTRACT RECOMMENDATION, not enforced (core
reads no plugin semantics, §0). In a close the only irreversible action is the tracker's final push,
so it sorts last; delivery `close.post` teardown removes the worktree DIRECTORY (re-creatable from the
branch, hence rollback-safe) while `work/<id>` deletion is deferred, non-transactional cleanup.

**Reference plugin — the scratch handoff** (the non-derivable counterpart to §11's derived example): a
plugin mirroring the ball to an external tracker that assigns its OWN id.
- `create.pre`: create the remote issue; write the returned id into the staged frontmatter (so the
  seal captures it) AND to `plugins/<name>/<bl-id>/id` (scratch covers the window before the
  frontmatter write — the one genuinely non-derivable gap).
- `close.post`: read the id from the sealed trailer (derivable — no scratch needed here), transition
  the remote issue, then delete the scratch dir.
- `rollback create.pre`: read the id from scratch/worktree, best-effort delete the remote issue,
  remove the scratch dir. Idempotent: absent ⇒ nothing created ⇒ no-op.

**sync/prime have no change worktree** (§13): tier 1 is empty (no ball seal), so rollback reduces to
each plugin's own `rollback`. Most sync/prime plugins are idempotent refreshers (no-op rollback); the
tracker's ff is a local ref move with nothing pushed, so a partial sync leaves the store at the old
or the new HEAD, never wedged — re-running converges.

## §15 Open topics (epic bl-b465)

Each becomes a § edit here when settled. **None open** — every topic resolved into the body.

RESOLVED (folded into the body, no longer open):
- **`drop` verb DELETED — it was `unclaim ∘ close`, and a close-gate bypass (2026-06-09, bl-65e0 —
  post-freeze).** drop was a composite, not a primitive: identical `Retire` mechanics to close,
  differing only in which §10 guard ran — and that difference was an enforcement hole: a `--blocks
  close` gate was unenforceable while drop existed, because dropping deleted the file exactly like
  closing it, gate unchecked. The §9 "kept as distinct ops because the op NAME is the hook-dispatch
  key" rationale never applied to drop's other half either — every plugin wired `drop.post`
  identically to `unclaim.post`/`close.post` (release/teardown), so the distinct name bought no
  distinct dispatch. RESOLVED by SUBTRACTION: the verb is gone; abandonment is the composite spelled
  out — `unclaim` then `close`, the empty deliverable making the delivery a no-op. Deliberate delta:
  work COMMITTED on `work/<id>` survives unclaim (branch deletion is deferred to prime, §14) and a
  later close DELIVERS it where drop discarded it — that is close honoring its contract; discard is
  an explicit `git branch -D work/<id>`. Legacy `bl-op: drop` deletions stay valid bedrock (read as
  closed; the deleting op was never reconstructed, §5). The forge contract's abandon-teardown moves
  `drop.post` → `unclaim.post` (spec-level only — no shipped plugin ever dispatched on drop). Touched §0/§2/§3 (closed is the one absence), §5 (op lists,
  no two terminal flavors), §6 (hooks table), §8 (symmetric family), §9 (five deliverable verbs;
  drop paragraph replaced), §10 (resolved = file gone), §11/§14 (forge abandon = unclaim). Code:
  `src/verb.rs`, `src/change.rs` (`Retire` is close-only), `src/mutate.rs`, `src/delivery.rs`,
  `src/tracker.rs`, `default-config/plugins.toml`. Tracked under bl-72a8.
- **`bl plugin <name> <op> <phase>` — strike the never-built dispatch verb (2026-06-09, bl-587f —
  post-freeze).** §6 claimed the verb "is the canonical dispatch and balls dogfoods it"; neither half
  was ever real. No such verb exists (`Verb::ALL`, `src/verb.rs` — `bl plugin` is an unknown command,
  exit 2), and core has always spawned plugins DIRECTLY as `<bin> <op> <phase>` (`src/plugin.rs`),
  through no verb. The load-bearing property — subprocess uniformity — DOES hold, but it is carried by
  the SPAWN CONTRACT (one argv/env/stdin shape for every plugin, shipped or third-party), never by a
  user-invocable verb. The familiar fork — IMPLEMENT or STRIKE — resolves by SUBTRACTION: STRIKE. No
  concrete consumer exists, and the spec carries no aspirational verbs; hand-testing a wiring needs no
  dispatcher — it is just running the plugin binary by hand with the contract's argv. If a real
  consumer ever materializes, that is a fresh design, not a revival. Touched §6 (the dispatch
  paragraph now states the spawn contract). Doc only — no code change. Tracked under bl-72a8.
- **spec drift sweep — the CODE is the conformant side throughout (2026-06-09, bl-3911 —
  post-freeze).** A doc-only audit of places where shipped code and this spec had silently diverged;
  every correction re-derives the spec text from the build (`default-config/plugins.toml`,
  `src/seed.rs`, `src/verb.rs`). (1) §6's default hooks table had drifted from the seed by THREE rows:
  `prime.post` is `[bl-delivery, tracker]` — §12's own "prime/post settles the CONTENT (fetch-ff +
  push)" (bl-0a23) REQUIRES the tracker there, but bl-0a23's entry touched §12/§13 and never the
  table; `install.pre = [tracker]` (the §13 config fetch) was missing entirely; `show = [bl-delivery]`
  (the bl-0af4 read-op dispatch) was missing. Table now mirrors the seed verbatim. (2) §6's "reads
  stay plugin-free in PRACTICE only because nothing is listed for them by default" was FALSE since the
  `show` wiring shipped — reworded: the default schedule uses the `show` key; `list`/`dep-tree` are
  the ones nothing lists. (3) §12 specced seeding as a pure copy of `default-config/`, but
  `src/seed.rs` PRUNES hook entries whose binary is absent beside `bl` before committing (a
  tracker-less/test box never aborts) — defensible, now specced, with the boundary drawn: the prune is
  seed-time only; §6's dangling-`bin/` clean error still governs an established landing, and re-prime
  rebinds symlinks but never rewrites the committed schedule. (4) §13's `prime --install` prose never
  said HOW the center's config is fetched — added: the tracker's `install.pre` hook fetches
  `balls/config` into the landing's `FETCH_HEAD`, a read only, stealth no-op (matching
  `src/tracker/remote_ops.rs::fetch_config`). (5) Cosmetic: §9 spelled the verb `dep tree`; the token
  is `dep-tree` (`src/verb.rs`). And two §15 entries now carry the SUPERSEDED markers later entries
  had earned: bl-7d46(5)'s "`NN-` prefix is already the structural lever" (retired by bl-8540's list
  ordering) and the config/store split's "recursive `prime --install`" (retired by bl-7d46(1)'s
  single hop). Touched §6 (table + reads bullet), §9 (verb spelling; the bl-e196/bl-d31f entries below
  are this sweep's too), §12 (seed prune), §13 (install.pre fetch), §15 (two markers). Doc only — no
  code follow-up. Tracked under bl-72a8.
- **`update --edit` — the $EDITOR-sourced whole-buffer update; no new verb (2026-06-09, bl-e196,
  commit f5a92250 — post-freeze; shipped silent, recorded by bl-3911).** §9 gained the HUMAN
  projection of `update`: `--edit` renders the stored `tasks/<id>.md` to a temp buffer, blocks on
  `$EDITOR` (else `$VISUAL`), parse-validates the saved buffer as a whole ball (re-edit or abort on a
  parse failure — garbage is never committed; an unchanged buffer is a no-op), and hands it to the
  EXISTING update seal — never a new verb, never an unvalidated commit. Mutually exclusive with the
  field flags (they would race over one payload); interactive-only — non-tty stdin or unset editor is
  an ERROR, not a fallback, so agents keep the flag-driven path. The id is the path and `created` is
  history (neither hand-editable); `updated` is restamped by the seal. Touched §9 (update prose).
  Code: `src/mutate_edit.rs` (+ `mutate`/`mutate_build`/`change_field`). Tracked under bl-72a8.
- **`--` end-of-options separator on create/update (2026-06-09, bl-d31f, commit 087ac06f —
  post-freeze; shipped silent, recorded by bl-3911).** The §9 front-door parser honors the getopt
  `--` convention: everything after it is positional, so a caller passing an untrusted `-`-leading
  title cannot have it parsed as a flag — `bl create -- "$TITLE"`. Glued-short expansion (bl-d165)
  stops at the separator too: beyond it nothing is a flag, so nothing unglues. Touched §9 (update
  prose, alongside `--edit`). Code: `src/mutate_args.rs`. Tracked under bl-72a8.
- **`command` carries no `field_changes` — strike the never-populated changeset (2026-06-09, bl-3bfd —
  post-freeze, surfaced by the bl-004c minimalism review, cross-referenced by the bl-1a66 arch
  review).** §7's pre payload described `command` as `op` + intended `field_changes` + `body_change`,
  and the code defined the matching `FieldChange { field, value }` — but the only construction site
  (`src/mutate.rs`) always passed `Vec::new()`, so under the serde empty-skip the key never appeared on
  any real wire. The bl-667e fork again — POPULATE or STRIKE — and the same resolution, by SUBTRACTION:
  STRIKE it. The op's field-level diff already has a single authoritative home — the change worktree a
  `pre` plugin edits, plus the wire's op-start state (`current_state`/`previous_state`) to diff
  against — so a wire-carried changeset is a second representation of one fact, guaranteed to drift
  (§0 single source of truth). Populating it instead would have forced every verb to author a parallel
  diff description plugins can already derive. No plugin consumes it: the in-tree plugins (tracker,
  bl-delivery) and the github forge/issues plugins were grepped — no reader. `command` is now `op` +
  `body_change` (`body_change` stays: it is populated, authored intent). If a future plugin genuinely
  needs a core-computed changeset, that is a separate design, not a revival. Touched §7 (pre payload:
  struck `field_changes`, added the no-wire-changeset rule), §8 (the config/install task-shaped-fields
  aside), §13 (the absent-`command` aside). Code: `src/wire.rs` (`FieldChange` + `Command.field_changes`
  removed), `src/mutate.rs` (construction site). Tracked under bl-72a8.
- **post wire carries no `current_state` — strike, not populate (2026-06-08, bl-667e —
  post-freeze, surfaced by the bl-1a66 conformance review).** §7's post payload listed
  `previous_state`/`current_state`, but the build never populated the post `current_state` (the sealed
  after-state): `OpContext.after` was always `None`, so the field serialized as absent on every post
  call. Two ways to reconcile a spec field with code that never fills it — POPULATE or STRIKE.
  RESOLVED by SUBTRACTION (minimalism): STRIKE it. Populating correctly would force the generic §8
  engine to re-read the sealed ball and thread a `Task` onto the post wire — coupling the engine to
  task parsing — to carry a value §14 already says a `post` reactor must DERIVE, not be handed: "post
  never mutates the ball; derive-don't-store is what makes that safe… a value that only EXISTS
  post-seal is DERIVED on read." The landed ball is exactly such a value — the reactor has the sealed
  `commit` on the wire and reads the tree with `git show`. So the after-state was never a payload field
  in spirit; the wire now carries `previous_state` (the op-start ball) plus the commit pair, and the
  after-state is a git read. No shipped plugin consumed the field (the delivery plugin reads
  `current_state` only at `pre`, for the squash subject's title — untouched). Touched §7 (post payload:
  struck `current_state`, kept `previous_state`). Code: `src/wire.rs` (`OpContext.after` removed, the
  post branch nulls `current_state`). Tracked under bl-72a8.
- **status filtering unified under one axis — `--closed` subtracted (2026-06-08, bl-7218 —
  post-freeze, refines bl-d7a5 below).** bl-d7a5 had given `bl list` a standalone `--closed` flag
  alongside `--status`/`--all`. Two flags for one axis is the smell: `--closed` IS `--status closed`.
  RESOLVED by subtraction — `--closed` is gone; the §3 ladder is one predicate axis,
  `-s`/`--status {ready|blocked|claimed|closed}`, with `closed` INFERRING the dead-set reach (a short
  `-s` alias added for the common case). `--all` survives as the one orthogonal selector — live + dead,
  which no single `--status` value names (so `-s closed` and `--all` don't combine). Touched §9 (the
  `bl list` flag surface); the bl-d7a5 entry's `--closed` mention is superseded.
- **implicit `origin` discovery is the tracker's, from the project repo — not core's, off the landing
  (2026-06-08, bl-976b — post-freeze; code follow-up bl-a476).** Surfaced smoke-testing bl-0a23:
  federation only worked with an explicit `--remote`/XDG `remote`; a plain `git clone <proj>; bl prime`
  went stealth. Cause — `checkout::resolve_remote`'s bottom tier ran `git remote get-url origin` on the
  **landing**, which is local-only (§2 install-transport), founded by a bare `git init`, and never
  carries an origin (no production path `remote add`s one) — so the `origin` tier was DEAD, and §12's
  "standard case needs no install … out of the box" could not fire. Two corrections: (1) the implicit
  `origin` is the PROJECT repo's — `git remote get-url origin` on the **invocation path** (the cloned
  repo whose `origin` is the real remote, where `balls/tasks` sits alongside the code), NEVER the
  landing; (2) discovery belongs to the TRACKER, not core — core resolves only the explicit
  `--remote`/`--center`/XDG tiers (config reads) and hands the tracker `remote: None`, and the tracker
  discovers `origin` from the invocation path as its single shared fallback (not re-probed per handler).
  Folded into §12 (the "standard case" bullet gains the origin-source + ownership clarification). The
  code drift predates bl-0a23 (came in with `resolve_remote`, bl-cd21). The move into the tracker LANDED
  (bl-a476, cleanup bl-e060): core's `resolve_remote` is GONE — with the `origin` tier removed it was a
  bare explicit-config read, so it dissolved to the call sites (`bind` reads `cli.or(xdg_remote)`, a
  mutate op reads `xdg_remote` directly), making "core only reads explicit config, never resolves a
  remote" structural; core hands the tracker `remote: None` otherwise. The tracker discovers `origin` in
  ONE place —
  its `handle` dispatch point resolves the effective remote once (explicit binding `remote`, else
  `git remote get-url origin` on `invocation_path`) and writes it back onto the binding, so every handler
  reads `binding.remote` unchanged and the stealth gate ("no remote ⇒ no-op") now means "no explicit
  remote AND no discoverable origin" with no per-handler re-probe. Discovery is a stateless per-op pure
  read (mutate ops discover too, via their `*/post` push) — deterministic over the project repo, so all
  handlers agree with no persisted session state. Tracked under bl-72a8.
- **prime materializes the store LAZILY via a core fixpoint — supersedes the bl-fa00 reset (2026-06-08,
  bl-0a23 — post-freeze).** bl-fa00 had founded a throwaway orphan store EAGERLY (before any remote was
  resolved), so a fresh clone's local store had history UNRELATED to an established remote and the
  tracker had to `reset --hard` it onto the remote tip — reconciling a divergence after creating it.
  REFRAMED so the divergence is never created. Core's only store primitive is now
  `materialize(tasks_branch)` — "a branch is a disk path": ensure that branch has a worktree, founding a
  fresh orphan IFF the ref is absent (§2). `prime` founds the LANDING eagerly but runs a bounded,
  CORE-OWNED FIXPOINT for the store: loop `prime/pre` then `materialize` until the configured name stops
  moving, then `prime/post` once (§8/§12/§13). The tracker splits across the two slots: `prime/pre`
  settles the NAME (warn-only on a store-elsewhere mismatch — it NEVER rewrites `tasks_branch`, since
  config crosses only by `install`, §12) and CLONES IN an established remote branch into a local ref;
  `prime/post` settles the CONTENT (fetch-ff + push; the founding push moved here from the old in-pre
  handler). So materialize checks out the cloned-in ref before any orphan is founded — no divergence to
  reset. The loop's signal is the config branch core owns (no §7 return channel) and is bounded by the
  §6 invocation-tree cap; it converges in one `pre` pass when the dial holds (the common case). Touched
  §2 (lazy materialize bullet), §8 (prime is a fixpoint, not a single pre→post), §12 (two-slot tracker
  prime + lazy substrate + E5 now covers prime/post's established publish), §13 (prime fixpoint +
  `prime/pre` cwd = landing). Code: `src/substrate.rs` (`found_landing` + `materialize`),
  `src/lifecycle_diffless.rs` (`Engine::fixpoint`), `src/checkout.rs` (`converge`), `src/tracker/prime.rs`
  (`prime` clone-in + `prime_post`); the bl-fa00 `adopt`/`reset` is reverted. Tracked under bl-72a8.
- **worktree path is a staged frontmatter field; `delivered_in` stays derived (recency-ordered)
  (2026-06-07, bl-934a — post-freeze; part (1) SUPERSEDED by bl-0af4 2026-06-09, entry below — the
  field is deleted, the path is computed-and-printed; (2) stands).** A spec attack on §11's consumer
  interface: the worktree path
  is consumer-relevant (an agent needs it after `bl claim`), but §7 gives a plugin no return channel
  and §6 makes plugin stdout a forward-verbatim HUMAN channel interleaved across the whole chain — not
  reliably machine-parseable. RESOLVED in two halves that look like one pattern but are NOT.
  (1) **worktree path — STAGE it (a field). [SUPERSEDED 2026-06-09 by bl-0af4 — the field is DELETED;
  the path is computed-and-printed, never stored. The original reasoning is kept for the record; the
  reversal and its case are in the bl-0af4 entry below.]** The delivery plugin writes the derived path into a
  plugin-namespaced preserved frontmatter key (`delivery-worktree`, §3 seam) at **`claim.pre`**, so the
  seal captures it and `bl show --json` reads it authoritatively. `pre` not `post` (§14 bars a post
  reactor mutating the sealed ball); possible because the id already exists at claim (unlike create) and
  `claimant` is the field claim stages, so the path is fully determined pre-seal. NOT a derive-don't-store
  violation: the path is derivable BY THE PLUGIN but NOT by the consumer (core/agents lack the formula
  and the plugin's territory layout, §0), so the stored copy is the consumer's single deterministic
  source — written through §7's one sanctioned channel (a plugin contributes by editing frontmatter).
  The field TRAVELS WITH `claimant`: staged at `claim.pre`, cleared at `unclaim.pre` (new `*.pre`
  wirings, §6), mirroring core's claimant set/clear — present iff the ball is claimed, never naming a
  released worktree. Seal-consistency holds: a `claim.post` materialize failure post-aborts the seal
  (§8), so field-present ⟺ worktree-exists at every committed state. The plugin still RECOMPUTES the
  path for its own resource/rollback and never reads the stored copy, so its stateless property is
  intact (the key is purely consumer-facing). stdout stays the human hint.
  (2) **`delivered_in` — KEEP it derived (NOT a field).** The note floated making `delivered_in` the
  sibling field, written at `close.pre`. REJECTED — these are not the same pattern: `claim` keeps the
  ball file (a `claim.pre` write lands durably), but `close` seals the ball-file DELETION as one commit,
  so a `close.pre` frontmatter write is eaten by the deletion and never reaches a recoverable file-present
  tree (the §9 recency walk reads the PRE-close commit, which predates delivery). It is also unnecessary:
  the id-reuse grep ambiguity bl-d7a5 deferred here dissolves by RECENCY-ORDERING the derived query — a
  reused id only begins after the prior incarnation closed, so deliveries are monotonic with incarnations,
  and the k-th-most-recent incarnation maps to the k-th-most-recent `[bl-id]` integration commit (the same
  live-first-else-most-recent walk §9 uses). Derived + recency-ordered keeps no-field / no-staleness AND
  the disambiguation; §14's "post never mutates; `delivered_in` is a `git log --grep` query" worked
  example stays intact. Touched §6 (hooks table: `claim.pre`/`unclaim.pre`), §9 (claim/unclaim prose),
  §11 (the path-staging + the `delivered_in` recency note). Code follow-up (delivery plugin writes/clears
  the key on `claim.pre`/`unclaim.pre`; the derived query recency-orders) filed bl-ae51 under bl-72a8.
- **worktree path is computed-and-printed, never a field — supersedes bl-934a(1) (2026-06-09, bl-0af4
  — post-freeze).** bl-934a(1) staged the path into a `delivery-worktree` frontmatter key read via
  `bl show --json`. REVERSED. Three faults: (a) the path is a pure function of `(binding, id, claimant)`
  AND already a local git fact — `git worktree list` prints `work/<id> → path` — so the field was a THIRD
  copy of what git already owns (derive-don't-store, §0). (b) it is machine-LOCAL (`$XDG_STATE_HOME` +
  `invocation_path`), and the default delivery+tracker combo seals-and-PUSHES the store, so the field
  replicated a local path to every clone — meaningless to all but the one claimant on the claiming
  machine, who can compute it locally; the claim is shared truth (belongs on the remote), its worktree a
  local derivation (does not). (c) bl-934a(1)'s justification — "stdout is interleaved across the chain,
  not machine-parseable" — is false under the existing discipline: tracker and every other plugin write
  only stderr, so delivery is the SOLE stdout writer on claim/prime, and stdout = the verb's one product
  (as `create` prints the id) is deterministic. RESOLVED: store nothing. The delivery plugin PRINTS the
  path on `claim.post` (start work) and `prime.post` (resume), and answers `bl show` via a READ-OP
  dispatch folded into the HUMAN render; `bl show --json` does NOT dispatch and never carries the path
  (lossless store mirror; the worktree is not stored). This establishes that READ OPS MAY DISPATCH
  PLUGINS — never a deliberate prohibition, now documented. Touched §6 (drop `claim.pre`/`unclaim.pre`
  from the default hooks table + new "Reads may dispatch plugins" contract), §9 (claim/unclaim prose:
  `claim.post` prints, nothing staged/cleared), §11 ("Core never computes this path, and nothing stores
  it" + Hooks). Code follow-up (delivery: delete the `delivery-worktree` stage/clear — reverting the
  field half of bl-ae51; print on `prime.post`; add the `show` read-op; core: dispatch a plugin read on
  the `bl show` human render and drop the `claim.pre`/`unclaim.pre` default wirings; fix the stale `bl
  skill` line that says `--json` omits the key; close the ghost bl-b930) filed bl-9ccb
  under bl-72a8.
- **prime push-failure splits founding-miss vs established-reject (2026-06-07, bl-9857 —
  post-freeze).** §12 read "A push that fails for lack of perms falls back to stealth-local silently."
  That conflated two cases into one silent path. The fallback is defensible ONLY for FOUNDING — the
  remote store branch is ABSENT and you lack create perms, so "couldn't found, stayed local" is
  genuinely harmless and once-per-clone. It is WRONG for an ESTABLISHED remote: an upstream rejection of
  a push to an already-existing store (non-ff, perms revoked mid-life, server-hook reject) means your
  mutation did not land while you believe you are federated — a silent split-brain. RESOLVED by
  distinguishing at the tracker on the SAME `ls-remote` that already decides adopt-vs-found: branch
  absent → founding → silent stealth fallback (unchanged); branch present → established → ERROR (new
  catalog code **E5**), surfaced and aborting the op (the §13 pull→mutate→push contract), never a stealth
  degrade. The split falls out of the existing structure for free — `prime` is the only founder and never
  pushes to a present branch (adopt = no push), so the established-reject path is exactly every op's
  `*/post` publish, which already errored, while `prime`'s founding push already swallows its rejection
  into stealth. The two halves were therefore already realized in code (`src/tracker/prime.rs`'s founding
  fallback + `src/tracker/remote_ops.rs::push`'s erroring publish); this resolution makes the DISTINCTION
  explicit so it cannot silently regress — naming it in both call sites' docs and giving the established
  reject a catalog code. Touched §12 (rewrote the founding-fine paragraph + added E5 to the error
  catalog). Tracked under bl-72a8.
- **git log as content — recency-resolved id lookup; `ready`→`list --status ready`; `list` history
  filters; `show` fallthrough (2026-06-07, bl-d7a5 — post-freeze).** The build had drifted toward
  treating closed tasks as gone; the design intent was heavier *implicit* use of the log. Three changes
  under one discipline. (1) `bl ready` REMOVED — folded into `bl list --status ready`: one listing verb,
  composable filters, fewer verbs (subtraction). `--status {ready|blocked|claimed}` filters the derived
  3-state ladder (§3) the render already paints (a space-separated value, not a bespoke `--ready`
  boolean); `bl list` orders every listing by `priority` then `created`, so ready's ordering was never
  special. (2) `bl list` keeps defaulting to open but gains `--closed/--all` + date/`--tag`/text filters
  served from history — dead balls are older content, not gone. (3) `bl show <id>` falls through to
  history on a live miss, reconstructing the most recent incarnation. UNIFYING DISCIPLINE — **recency
  resolution**: every id→content lookup is live-file-first, else most-recent-in-history, stop at first
  hit. This DISSOLVES the id-reuse-vs-history concern (the planned "scan history to prevent reuse" ball
  is unwanted — at most one incarnation is live, so a reused id is unambiguous by construction); the
  4-hex space stays. The `delivered_in` cross-incarnation grep subtlety was struck as a conflation —
  it greps the DELIVERY log, an extension's concern, moved to bl-934a. Touched §2, §3, §4, §9, §10,
  § id generation. Code follow-ups (remove the `ready` verb, add `--status`/`--closed`/`--all` +
  history reconstruction in `show`/`list`) filed under bl-72a8.
- **recursion guard → general invocation-tree cap, fail+rollback (2026-06-07, bl-7110 — post-freeze).**
  §6 read: at the `BALLS_PLUGIN_DEPTH` cap "nested ops run PLUGIN-FREE (suppressed, not refused)" and "a
  plugin may deliberately re-enable plugins on a nested call." Two defects. (1) What the guard catches is
  almost always a BUG — self-retrigger, a plugin wired on op X whose handler shells `bl X` (a `sync.post`
  plugin running `bl sync`), climbing depth to the cap; the legit shell-back (forge `claim.post` →
  `bl create` for the gate child) goes 1 deep and never re-triggers its own op, so it never nears the
  cap. Running PLUGIN-FREE at the cap converts that runaway into QUIET wrong-behavior: the offending
  plugin gets no signal and the op silently runs without the plugins it expected — the worst disposition.
  (2) The re-enable hatch lets a runaway defeat its own backstop. RESOLVED, two moves: disposition FLIP
  (suppress → fail+rollback) and GENERALIZE. Cap-hit now ABORTS + rolls back (§8/§14 reverse-order) with a
  diagnostic naming the op/plugin chain, so the loop surfaces. And the cap is no longer plugin-specific:
  it is ONE odometer over the whole INVOCATION TREE — every nested `bl` bumps it whatever the source
  (plugin shell-back, op→op, clone→clone), so plugin-recursion-depth is just one DIMENSION and the same
  backstop also catches runaway clone-spawning. The hatch is DELETED; a one-line note discourages a
  plugin from re-triggering its own op (the §0 "plugins don't depend on each other's presence" already
  covers the mutual `plugin1`↔`plugin2` half). Bounded chains are unaffected (they never hit the cap);
  finer-grained per-op controls can layer on top — this is the failback under them. Touched §6 (rewrote
  the guard bullet). No code follow-up filed; enforcement lives in core dispatch/run alongside the
  existing depth env.
- **`status` plugin SUBTRACTED — bedrock vs render projection (2026-06-07, bl-d074 — post-freeze).** The
  topic proposed a shipped default `status` plugin that persists a 4-value field (`open`/`blocked`/
  `in_progress`/`in_review`) and keeps it fresh by **cross-task fan-out** (`close.post`/`drop.post`
  re-sealing every claim-blocker dependent — the system's first write-amplification), to spare external
  integrators reimplementing core's derived ladder (§3) when they read `--json`/raw files. RESOLVED by
  SUBTRACTION: the real gap was a *use-case conflation*, not a missing denormalization. `--json` exists
  to expose **bedrock** — the lossless, round-trippable mirror of stored state ("what's actually
  there") — so injecting a derived `status` into it defeats its one job. The two needs are two
  orthogonal PROJECTIONS the verbs already embody: the DEFAULT human render (`bl show`/`list`)
  paints derived columns (the status ladder, tree, ISO dates); `--json` stays bedrock. A machine
  integrator reads bedrock `--json` (`claimant` + `blockers`, already present) and runs the same ~6-line
  ladder core runs — so no stored field, no fan-out, no drift, no backfill, no plugin. The only residue
  (the gate/PR `in_review` 4th value core folds into "claimed", §3, and a *stored ordered pipeline*)
  stays the §3 opt-in seam — a preserved `state:` key + a team's own display plugin, severable, never
  default. The denormalization-cache (which the ball itself admitted "a WRONG cache is worse than none")
  is gone; SSOT (the derived ladder, §3) holds. Touched §3/§9 (named the bedrock-vs-render split
  explicitly). No code follow-up — the subtraction removes a build (it was gated on bl-20fc/bl-4e14).
- **plugin schedule is config, not the filesystem (2026-06-07, bl-8540 — post-freeze).** FLIPS the §4
  line "There is likewise no `plugins` config list — the filesystem symlink registry (§6) IS the plugin
  chain" 180°. The trigger: the symlink registry (`config/plugins/<op>/<phase>/NN-<name>` →
  `../bin/<name>`) was a SECOND config mechanism running parallel to §4's `balls.toml` layering, and it
  had reinvented an ordered list badly — run order encoded as a `NN-` string-prefix on filenames, the
  sysvinit/`/etc/rc.d` tic. But §4 ALREADY has ordered lists with merge directives (`compose_list` /
  `_prepend`/`_append`/`_ban`, built + tested in `config.rs`). RESOLUTION by SUBTRACTION:
  (1) the HOOK SCHEDULE moves into a committed, §4-layered `config/plugins.toml` — a `[hooks]` table
  mapping `<op>.<phase>` → an ORDERED LIST of plugin names; **list position = run order** (the last
  name runs last, so the §8/§14 "irreversible push sorts LAST" is just "tracker is last in the list").
  An absent/empty list = run nothing. It layers + merges exactly like every other config field, so a
  center's schedule composes with an XDG `_prepend`/`_append`/`_ban` and travels by `bl install` — ONE
  layering mechanism, not two.
  (2) the BIN folder is UNCHANGED — `config/plugins/bin/<name>` machine-local gitignored symlinks;
  install drops a symlink, dispatch resolves a hooked NAME to it, a dangling `bin/<name>` is the same
  clean "referenced but not installed here" error. Schedule (committed text) and binary (local symlink)
  now split on file-vs-symlink instead of two symlink levels.
  (3) `install`'s plugin-wiring-mirror collapses — `install plugins.toml` is an ordinary config copy;
  no symlink-tree walk, no `Object::Plugins` special path beyond binding+protocol-validating the named
  binaries. WHAT WE LOSE (accepted): sparse mid-phase insertion. `NN-` was a sparse integer keyspace
  (slot in at `70` between `50` and `90`); a positional list with `_prepend`/`_append` only lands at an
  END of a phase — mid-insertion needs a full-list replacement. The one HARD constraint (tracker's push
  LAST) holds because `_prepend` covers "run before the push." No load-bearing principle breaks: §0
  "core knows a plugin's name + binary, never its config" holds (the schedule is CORE's config, not the
  plugin's); §6 "sequence of binaries, run-parts" becomes "sequence of binaries, list-ordered" — same
  semantics. Touched §0/§2/§4/§6/§8/§9/§12/§13/§14/§16. Implementation: registry→config conversion in
  `registry.rs`/`install.rs`/`config.rs`/`substrate.rs`/`mutate.rs`/`checkout.rs` + delivery wiring,
  and bl-4e14's seed collapses to a plain-text `plugins.toml` (`include_str!`, no `build.rs`/symlink
  embedding). Tracked under bl-72a8.
- **observability — logs & metrics (2026-06-06, bl-b58a — post-freeze).** A topic raised AFTER the
  freeze (so the "None open" above had missed it): the doc carried three divergent log conventions —
  §1 `logs/<name>/plugin.log` "suggested; plugins do as they please", §6 "stderr captured to
  clones/<enc>/logs/<name>/", and §14's rollback echo — while the code (bl-5d56) had invented a fourth,
  per-op-phase `logs/<name>/<op>-<phase>.log`. RESOLUTION by reframe + subtraction:
  (1) ONE unified per-clone log `clones/<enc>/log` (§1) — JSON-lines, balls-owned, the source a stamped
  FIELD not a directory split, so you grep one source or read the whole sequence. Core emits its own
  lifecycle records AND envelopes each plugin stderr line; the plugin stays dumb (no `BALLS_LOG_DIR`).
  (2) `log_level` (§4) is the single write-time threshold over both file and terminal; a non-zero
  plugin exit emits a core `error` record that survives any threshold (§6).
  (3) METRICS CUT from core entirely (§6) — the log is the event stream and §5 trailers the history;
  metrics are a query or a `*.post` plugin, never core storage, no new seam. Touched §1/§4/§6/§14.
  Code reconciliation (replace bl-5d56's per-op-phase capture; delete dead `layout::log()`) is bl-2e9f.
- **doctor SUBTRACTED (2026-06-06, bl-77a7 — post-freeze).** The old §16 was `bl doctor`, a read-op
  drift-scanner (resolved earlier under bl-e8a5). Cut ENTIRELY — verb and section. The test it failed:
  given an agent holding the skill doc and a doctor-covered scenario, doctor's existence does not
  meaningfully raise fix-success. 5 of its 8 checks (stale change worktree, unresolved `tasks_branch`,
  unparseable config, missing claimed-ball worktree, stale store) fail LOUD already — git or the next
  op surfaces them, and an agent's reflex is `git status`/`ls`, not a tool-specific scan verb. The 3
  SILENT checks move to POINT-OF-USE errors that name the fix verb: protocol drift is already rejected
  at install (§6, the binary's `protocol` self-describe is validated before binding); a missing local
  `bin/<name>` fails at the op that needs the plugin ("bin/<name> missing — run `bl install`"); a
  blocker cycle is inert — readiness is immediate-only, so it never drives a recursive walk, just a
  mutual permanent-block that `claim` refuses by naming the blocker, `bl update` unlinks (§10). A
  fixed checklist enumerates only KNOWN drift, so doctor helped least on the weird unenumerated cases
  agents actually get stuck on — and an agent only reaches for it once already debugging, when it is
  already exploring git+files. The no-repair-verb principle (every fix is an existing idempotent verb —
  `prime`/`install`/`sync`/`update`) survives without a doctor section; §16 migration's tail now routes
  residual drift to point-of-use. Touched §1/§8/§9/§10/§11; deleted old §16 and scrubbed its
  cross-refs, renumbered §17 migration→§16. Code removal is bl-a38e.
- **coherence pass (2026-06-06, bl-7d46 — post-freeze).** An adversarial read of the just-frozen doc
  (bl-cac0) fixed defects the config/store split and the original drafting left behind:
  (1) §13 `prime --install` still described the RETIRED config CHAIN ("recursively down the config
  chain … until no further redirect"). Collapsed to a SINGLE-hop install (prime → install → prime): a
  center's config is self-contained and config crosses a boundary exactly once (§4/§12).
  (2) "Landing config is the SOLE authority" (§0/§4) overstated — the effective config is the LOCAL
  stack landing ⊕ XDG ⊕ CLI (§4 read order); reworded to "no REMOTE is authoritative."
  (3) §11 PRIVILEGED the delivery plugin: core computed+printed its worktree path, baking the plugin's
  name + territory layout into core (vs §0/§6 "no privileged plugins; core knows only name+binary").
  Fixed by de-privileging — the plugin PRINTS its own path on its stdout, which §6 now defines as the
  plugin's user-facing channel that balls forwards verbatim and parses nothing from; "claim prints the
  path" is the plugin printing, not core knowing. The derived path may also key on `claimant` (already
  on the wire — keeps the stateless-recompute property; disambiguates a reclaim by a new actor).
  (4) §8 "every op is the same shape" was oversold. Reframed to TWO families: task ops (the symmetric
  sealing shape) vs the rest, which inherit partially — `config`/`install` seal to the landing; `sync`/
  `prime` author no diff, inherit only the pre→post spine + rollback, and `sync`'s integration
  is the tracker's ff (a plugin's, not core's).
  (5) Soft spots made structural where cheap: landing single-owner is now "balls cannot publish it"
  (only the tracker pushes, only `tasks_branch`; raw `git push` by hand is the only residue, §4);
  `install`'s folder=wipe blast radius is mitigated by an `N added/M deleted` summary + git-recoverable
  commit, not a flag (§6); §14 sort-last left as-is (the `NN-` prefix is already the structural lever)
  [SUPERSEDED 2026-06-07 by bl-8540, entry above — the `NN-` filename registry retired into the
  `[hooks]` list; sort-last's structural lever is now LIST POSITION (the last name runs last)].
  (6) §3/§10 blocker model — generalized + de-conflated (the late-subtask "gap" was a mis-frame, not a
  missing invariant). `on` is now ANY op (claim/close sugared, §3), and **containment is fully separate
  from blocking**: `--parent` sets the tree pointer ONLY and gates nothing; blocking is always explicit
  via `--blocks OP` (gate the parent's — or `ID:OP`'s — op) or `--needs ID[:OP]` (gated-by, default
  claim). `--gates` retires into `--parent X --blocks close`; an epic is pure containment that gates
  nothing by itself; closing a task with live children is allowed (their `parent:` dangles, display-
  only). The standard subtask/gate/review patterns are skill-doc hints, never core rules. This was the
  load-bearing subtraction — every edge is explicit and self-describing, and the deadlock-reciprocal
  and late-add gaps both dissolve because nothing is auto-minted. Touched §3/§9/§10/§16. bl-7d46 fully
  resolved.
- **config/store split (2026-06-05, post-finalization revision — supersedes parts of bl-62bc and
  bl-0601).** The trigger: config-shadowing (§4 values layering down the trail) and `install` were two
  mechanisms for one job — propagating a center's config — and they CONTRADICTED on the threat model.
  install exists to gate config adoption with consent; shadowing adopted it automatically at read,
  leaking around install. The "config is inert data, not code" defense was too weak (a shadowed
  `branch`/`tasks_branch` redirects where you write). RESOLUTION by SUBTRACTION + a reframe:
  (1) kill trail config-layering — config is read only from the landing (+ XDG + defaults, both
  local-trusted); a center's config crosses only by explicit `install`, materialized once (§4).
  (2) SPLIT config and tasks onto two branches (`balls/config` + `balls/tasks`), `config/`/`tasks/`
  top-level folders always — this makes the transport asymmetry STRUCTURAL (config = single-owner,
  destructive install-replace; store = shared, sync-merge) instead of disciplinary, and DISSOLVES the
  whole trail/terminus/`operating`/stealth-mode apparatus: config NAMES the store via `tasks_branch`,
  one indirection that subsumes find-store + sync-target + install-source. Chains
  and transitive auto-discovery go (a layering artifact; one shared store needs no transitivity).
  "Stealth" becomes a `tasks_branch` value, not a mode. (3) `install` is **pure path-copy** —
  `install <path>` makes dest/`<path>` == source/`<path>`, siblings untouched; folder = mirror
  (deletions propagate), file/glob = union (overwrite, no conflict-detect, git recovers). No object
  enum, no merge-vs-replace logic; per-task install falls out free. The store-deletion-resurrection
  worry dissolves by ADDRESSING (use the folder form), not a tombstone mechanism. (4) **landing config
  is the SOLE authority** for what runs + where it syncs, changed only by user command; **ALL config is
  potential RCE** and crosses only by `install` — no "safe data auto-flows" carve-out. (5) **No
  run-time defaults:** a fresh landing is SEEDED at prime by copying the app `default-config/` folder
  (§1), so tracker/delivery/builtin plugins are ordinary landing entries — swap the folder to swap the
  default set (policy in config, not core code). (6) standard case (seeded default `origin` +
  `balls/tasks`) works `prime; list` out of the box; non-default needs `install` (or recursive
  `prime --install`, stop-on-revisit) [SUPERSEDED 2026-06-06 by bl-7d46(1), entry above —
  `prime --install` is a SINGLE hop (prime → install → prime), never recursive; a center's config is
  self-contained, no chain to walk]; a tracker WARNING (not silence) fires when landing is on the
  seeded default but `origin:balls/config` names another store. (7) implicit founding is fine (bare
  prime create+push; `--stealth` opts out; no-perms → stealth fallback). Footprint cost = +1 (cheap,
  local) ref; mechanism deleted >> ref added. Reuse of one branch for both roles is legal
  (folder-namespaced) but a config ref shared AS config between clones corrupts (no merge) — so the
  landing is single-owner-by-discipline (§4). Touched §0/§1/§2/§4/§6/§7/§8/§12/§13/§16; build
  corrections under bl-72a8.
- **bl-cef0** migration from legacy balls — RESOLVED into **§16**. A one-shot throwaway *script*
  (not a verb — same retirement as init→prime / review→close / remaster), splitting base-field
  migration from per-plugin state migration on the §14 plugin-territory boundary, governed by
  migrate-clean-or-delink. Re-admits `priority` to core (§3/§9/§10). See §16.
- **bl-4778** states-as-config — RESOLVED by SUBTRACTION: there is **no `state`/`status` field at
  all**. Status is a DERIVED VIEW computed by a short-circuit ladder over exactly THREE live states:
  `claimant` set ⇒ **claimed** (claim-blockers not even evaluated); else unresolved claim-blocker ⇒
  **blocked**; else ⇒ **ready** (= the `ready()` predicate §10). "blocked" is claim-blockers ONLY —
  a close-blocker yields no status (a claimed ball with an open gate is just *claimed*), which is the
  same reason `review` was abolished (it was never functional). closed/dropped are absence, not
  states (file deleted + `bl-op`, §5/§9). A stored status would have zero core behavior (`ready()`
  never reads it), making it a single-valued tag — so it folds away; non-deriving human intent
  (`deferred`/`triage`) is a `tag`. The whole config cluster goes with it: no `state`
  field, **no `default_state`, no `states` vocab list, no per-op target knob** (§4), and **no
  `bl-from-state`/`bl-to-state` trailers** (§5 — `bl-op` names the transition). occupancy is the
  `claimant` field, the one structured fact the claim-guard reads (§0/§3). A team wanting a stored,
  ordered pipeline opts in OUTSIDE core via an unknown preserved `state:` key + a display plugin
  (§3) — severable, never a core field. Touched §0/§3/§4/§5/§9.
- **log line bound (2026-06-07, bl-e6a0 — post-freeze, correctness on bl-2e9f/bl-b58a).** A spec
  attack on §1's atomicity sentence: "One object per line keeps concurrent appends … atomic
  (sub-`PIPE_BUF`, `O_APPEND`)" was a CONDITIONAL guarantee left unenforced. `PIPE_BUF` (4096 on
  Linux) is the largest write the kernel won't interleave, and the §6 enveloped plugin-stderr `msg`
  is unbounded — a stack trace, a long git error, a diff line. The exact records the log exists to
  capture are the ones that overflow, and the failure mode (parallel agents + a verbose/failing
  plugin) is the design's stated use case. RESOLUTION: enforce the bound the claim already makes,
  rather than weaken the claim. Every serialized line is held at or below `LINE_MAX` = `PIPE_BUF`;
  `msg` (the only unbounded field) is truncated on a `char` boundary with a `…[truncated]` marker
  until the whole line fits. The lock-free `O_APPEND` fast path is KEPT (no `flock`, the rejected
  option 2 — it would contradict the "sub-`PIPE_BUF`" framing and add a mechanism); truncation
  re-serializes to measure real JSON escaping rather than reimplementing serde's escape table (single
  source of that truth). Lossy on giant lines by design — logs shouldn't carry them. Touched §1;
  code is `src/log.rs` (`Log::line`).

## §16 Migration from legacy balls

Legacy balls (pre-greenfield): task JSON on the `balls/tasks` orphan branch, plugin state inline in
the core JSON, a pile of config knobs, `[bl-xxxx]` tags on `main`. Greenfield: §2 markdown `tasks/`
on `balls/tasks` + config on `balls/config`, §1 XDG, §6 hook-list plugins. Migration is a **one-shot,
throwaway transform SCRIPT — not a verb.** A `bl migrate` would be the §0 "new verb is a smell" for a
job that runs once over a handful of known repos (`init`→`prime`, `review`→`close`, `repair`→verbs,
`remaster` all retired the same way). The script does ONLY the irreducible format transform;
everything ongoing — XDG bootstrap, worktree re-materialization — is already `bl prime`'s idempotent
job (§12/§13), so the script ends by handing off to `prime`.

**Governing principle — migrate-clean-or-delink, never guess.** The script transforms only what maps
deterministically and DELINKS anything it cannot prove (an unresolvable reference, a plugin's private
state); reconstruction is deferred to the authoritative source — the plugin's own adoption (below).
Single-source-of-truth applied to migration: never fabricate a mapping
the transform can't derive.

**Split on the plugin-territory boundary** (the same plugin-territory boundary as §14 scratch):
migration is NOT one script but **base-migrates-core PLUS each-plugin-migrates-its-own-territory.**
- **Base migrator** owns core fields only (§3 schema); it never reads plugin state.
- **Each plugin** owns its legacy state: its greenfield port carries a one-time *legacy-adoption*
  that seeds its §14 XDG scratch from the old inline blob.

**Core field mapping (base migrator).** Read `balls/tasks:.balls/tasks/*.json`; skip `status: closed`
(closed = file-absent, §9); write `tasks/<id>.md`:
- direct: `claimed_by`→`claimant`, `created_at`→`created`, `updated_at`→`updated`, `parent`,
  `priority`, `tags`; `description`→ the markdown body.
- `depends_on: [id]` → `blockers: [{id, on: claim}]`.
- `type: epic` → `tags += epic`; `status: deferred` → `tags += deferred` (§3 — both are tags;
  expect deferred balls to surface in `bl list --status ready`, which is intended, not a regression).
- **epic reciprocal edge (a reconstruction, not a rename):** for each LIVE child, add
  `{child, on: claim}` to its parent's `blockers` (§10). Legacy stored only the child→parent pointer
  and derived the rest from `status`/`closed_children`. Greenfield `parent` is containment ONLY and
  implies no edge (§10), so the migrator must mint this claim-blocker explicitly to preserve legacy
  "epic waits on its children" — without it the epic migrates spuriously `ready`. (This is the one
  place the migrator re-creates an edge the old implicit model derived; it is `--blocks claim` per
  child, done in the transform.) Closed children are skipped (absent file = resolved); a live child
  whose parent did not migrate has its now-dangling `parent:` nulled.
- **dropped** (no core home): `id` (= filename, §3); `status`/`delivered_in`/`branch`/
  `closed_children` (derived, §3/§11); `repo` (the store knows it, §12); `type` (folded to tag);
  `links` (legacy-unused); `external`/`synced_at` (plugin territory — below).

**Config mapping — most knobs dissolve** (`.balls/config.json` → `config/balls.toml` on `balls/config`,
§4): `id_length` is dropped (the id scheme is fixed — a custom scheme is a `create/pre` plugin, not
config); the remote becomes the tracker's; `tasks_branch` defaults to
`balls/tasks` (§4). Gone entirely — `version`, `worktree_dir` (path derived, §11), `protected_main`
(nothing-on-main is structural, §0), `auto_fetch_on_ready` (sync-every-op default, §13),
`stale_threshold_seconds` (→ the tracker's own §1 territory, not core). The legacy
`plugins: { name: {enabled, config_file} }` map → the §6 hook list: `config/plugins.toml` `[hooks]`
entries naming each enabled plugin in the op-phases it handles (op/phase read from `<name> protocol`),
local gitignored `bin/<name>`, and each `config_file`'s content → `config/plugins/<name>/`.

**Plugin state (the per-plugin half).** Legacy plugin state lived inline in core
(`external.<plugin>.*`, `synced_at.<plugin>`). The base migrator DROPS it; the plugin's greenfield
port re-adopts. Worked example — **github-issues**: its greenfield join SSOT is the `[bl-xxxx]`
marker on the issue *title* (its territory base is only a rebuildable sync-cache), but legacy issues
carry no marker, so a naive drop would unmatch on the next sync. Its port's one-time adoption amends
the SSOT directly: online, it appends `[bl-id]` to each legacy issue's title from the old
`external.github-issues.issue.number` (one PATCH per issue), so the next `sync` re-adopts via the
marker with ZERO dup. Because the marker is on GitHub, EVERY clone joins from it — federation needs no
per-machine state, and a clone that never ran the adoption still re-adopts with no dup. Skip the
adoption entirely and the cost is bounded — ONE dup per task on first sync (the plugin re-records and
stabilizes; never runaway) — the accepted floor of migrate-clean-or-delink, not a failure.

**Branch & history.** Greenfield uses TWO branches — `balls/config` (landing) + `balls/tasks` (store);
legacy used `balls/tasks` for the JSON store, so the store branch NAME collides. The script writes a
fresh orphan `balls/config` (seed config incl. `tasks_branch`) and migrates the legacy JSON into a
fresh greenfield `tasks/` on `balls/tasks`. Cutting a shared `origin/balls/tasks` over is a one-time,
human-coordinated migration: `bl install` writes the greenfield store and its per-op store sync (§12,
on by default) pushes it — no separate push step. The push may force-rewrite the shared ref; that is
intrinsic to a format change, not a thing core guards (git is the recovery net, §6 — `git branch
balls-archive origin/balls/tasks` first if you want the legacy history kept locally). The cutover
runbook is bl-0802, not a core-format concern. `main`'s legacy `[bl-xxxx]` commit-subject
tags stay untouched: forward-compatible with §11's delivery tag, so the `delivered_in` query (§11)
works over old history for free.

**Preconditions / guards.** One-shot, NOT idempotent-converge (that is `prime`'s contract): the
script REFUSES if `balls/config` already exists, and REFUSES if any live task is `claimed` (a claimed task's
in-flight code worktree would be stranded when `prime` re-materializes a fresh `work/<id>` — quiesce
first: merge/close/unclaim in-flight work before migrating). Sequence: run the script → `bl prime`
brings XDG/worktrees/config to readiness (§12) → each plugin's port runs its one-time adoption.
Any residual drift surfaces at point-of-use: the next op that needs a missing or stale piece fails
naming the verb that fixes it (`prime` re-materializes, `install` re-resolves a `bin/`, `sync`
refreshes the store, `update` unlinks a bad edge) — there is no separate scan verb.
