# E2E demo — adoption & §16 legacy migration (bl-d754)

> **Superseded mechanism (bl-e614):** the `migrate-legacy.py` script this
> transcript drives is deleted — the data step is now `bl import --legacy`
> (preview: `bl list --legacy`); see `docs/migration-runbook.md`. The §16
> guarantees demonstrated here (branch collision, epic edges, gh-adopt, the
> quiesce guard — now a runbook line) still hold; only the vehicle changed.

A captured live run of the legacy→greenfield cutover (architecture.md **§16
Migration from legacy balls**) plus the github-issues **`adopt`** step, against
**freshly-built binaries** (`cargo build --release` in the claimed worktree for
`bl`/`tracker`/`bl-delivery`; a fresh `main` checkout of the github-issues
plugin) in a throwaway `/tmp` repo with an isolated `XDG_STATE_HOME` — so the
demo never touches this repo's own task list (epic bl-9369's standing warning).

This scenario proves the four documented **migration holes** are actually
closed, and characterizes the **federation caveat** the task names:

| # | Hole | §16 guarantee | Shown in |
|---|------|---------------|----------|
| 1 | **branch collision** | greenfield `balls/tasks` (markdown) force-rewrites the legacy `balls/tasks` (JSON); `balls/config` is a fresh sibling | §4 |
| 2 | **epic edges** | a migrated epic mints a reciprocal `claim`-blocker per live child, so it stays `blocked`, not spuriously `ready` | §5, §6 |
| 3 | **gh dup** | `adopt` stamps the `[bl-xxxx]` title marker so the first `sync` joins, never auto-creating a duplicate | §7 |
| 4 | **claimed-guard** | the one-shot script *refuses* if any live task is `claimed` (an in-flight worktree would be stranded) | §2 |

**Federation caveat (the bl-0ef9 finding).** Because the join SSOT is the marker
*on GitHub*, a federated clone that **skips `adopt`** still joins via the marker
on its first `sync` — the old per-clone/per-machine join state is gone. The only
residual cost is skipping `adopt` **entirely** (no machine ever runs it): every
issue stays marker-less and the first `sync` auto-creates **one** duplicate per
task — bounded and self-stabilizing, the accepted floor of migrate-clean-or-
delink (§16). Shown in §8.

> The JSON op-log lines (`{"ts":…}`, on **stderr**) are elided with `2>/dev/null`
> for readability, as in demonstration.md.

---

## 0. Setup — fresh binaries + a legacy store

The throwaway env: fresh binaries on `PATH`, an isolated `HOME`/`XDG_STATE_HOME`,
and a legacy balls repo. Legacy layout: inline task JSON under `.balls/tasks/` on
an orphan `balls/tasks` branch, a `.balls/config.json` pile of knobs on `main`,
and `[bl-xxxx]` commit-subject tags. The fixtures exercise every transform:

- `bl-ep01` — an **epic**, `open`, with two live children (epic-edge case).
- `bl-ch01` — live child, `depends_on: [bl-be99]`, mirrored to GitHub issue 101.
- `bl-ch02` — live child, **`claimed_by: alice`** (the claimed-guard trip), issue 102.
- `bl-be99` — the backend dependency, issue 103, with a `notes.jsonl` sidecar.
- `bl-dead` — **`closed`** (must be skipped), issue 199.
- `bl-orph` — **`deferred`**, parent is the *closed* `bl-dead` (dangling → nulled), issue 104.

## 1. The transform self-test — `migrate-legacy.py --self-test`

The script is a **throwaway transform, not a `bl` verb** (§16: a verb for a
once-over-known-repos job is the §0 "new verb is a smell"). Its pure-transform
self-test runs with no repo:

```console
$ python3 migrate-legacy.py --self-test
self-test: OK
```

## 2. Hole 4 — the claimed-guard refuses in-flight work

`bl-ch02` is `claimed`. The script is one-shot, **not** idempotent-converge
(that is `prime`'s contract), and refuses rather than strand the claimant's
worktree when `prime` later re-materializes a fresh `work/<id>`:

```console
$ python3 migrate-legacy.py --repo $LEGACY --dry-run
refusing: live tasks are claimed (quiesce first): bl-ch02
(re-run with --force only if you understand the consequence)
$ echo $?
1
```

Nothing was written. **Quiesce** (merge/close/unclaim in flight) — here we drop
the claim on `bl-ch02` — and the dry-run passes, reporting the migrated shape:

```console
$ python3 migrate-legacy.py --repo $LEGACY --dry-run
legacy: 6 tasks (5 live) → greenfield: 5 migrated
  bl-be99  Auth backend  tags=backend
  bl-ch01  Wire the login child  parent=bl-ep01 blockers=1 tags=frontend
  bl-ch02  Session cookies  parent=bl-ep01
  bl-ep01  Auth epic  blockers=2 tags=auth,epic
  bl-orph  Revisit the spike  tags=deferred
```

`bl-dead` (closed) is gone (file-absent = resolved, §9); `bl-ep01` carries
**2 blockers** (one per live child); `bl-orph` is `deferred`-tagged with no parent.

## 3 & 4. Hole 1 — the branch collision (`--build-refs`)

Greenfield uses **two** branches; legacy used `balls/tasks` for its JSON store,
so the store name **collides**. The publish-first path builds both greenfield
branches as staging refs without clobbering the live ones:

```console
$ python3 migrate-legacy.py --repo $LEGACY --config-src ./default-config \
      --out $OUT --build-refs
legacy: 6 tasks (5 live) → greenfield: 5 migrated
  ...
wrote greenfield tree → /tmp/bl-demo-d754/tree
built refs/migrate/balls-config  e5ceec0…
built refs/migrate/balls-tasks   ddb717b…
next: see the RUNBOOK at the top of this script (push, then `bl prime`)
```

The collision, made explicit:

```console
$ # legacy store branch (JSON) vs the greenfield staging refs
$ git -C $LEGACY for-each-ref --format='  %(refname)  %(objectname:short)' refs/migrate/
  refs/migrate/balls-config  e5ceec0
  refs/migrate/balls-tasks  ddb717b

$ # greenfield balls-tasks is MARKDOWN — it would FORCE-overwrite origin/balls/tasks
$ git -C $LEGACY ls-tree -r --name-only refs/migrate/balls-tasks
tasks/.gitkeep
tasks/bl-be99.md
tasks/bl-ch01.md
tasks/bl-ch02.md
tasks/bl-ep01.md
tasks/bl-orph.md

$ # balls-config is a FRESH sibling landing — no collision
$ git -C $LEGACY ls-tree -r --name-only refs/migrate/balls-config
.gitignore
config/balls.toml
config/plugins.toml
```

Cutting `origin/balls/tasks` over is the human-coordinated step. At the time
of this demo (bl-0802's runbook) that meant `git branch balls-archive
origin/balls/tasks` to keep the legacy history, then a one-time **force**-push
of the markdown store. *Superseded by bl-8660:* runbook step 5 is now a
history JOIN — merge the legacy tip into the greenfield store with `-s ours`,
then an ordinary fast-forward push — so nothing is rewritten and the legacy
history stays in-branch (no `balls-archive`). `main`'s legacy `[bl-xxxx]`
commit tags stay untouched either way (forward-compatible with §11's delivery
tag).

## 5. The migrated markdown — every transform, in the files

```console
$ cat $OUT/tasks/bl-ep01.md          # epic: a reciprocal claim-blocker per LIVE child
+++
title = "Auth epic"
created = 1777629600
updated = 1777716000
priority = 1
tags = ["auth", "epic"]              # type=epic folded to a tag (§3)

[[blockers]]
id = "bl-ch01"
on = "claim"

[[blockers]]
id = "bl-ch02"
on = "claim"
+++
Umbrella for the auth work.
```

```console
$ cat $OUT/tasks/bl-orph.md          # deferred→tag; dangling parent (bl-dead closed) NULLED
+++
title = "Revisit the spike"
created = 1776243600
updated = 1776330000
priority = 3
tags = ["deferred"]                  # no `parent =` line — the closed parent was dropped
+++
Maybe revive.
```

```console
$ cat $OUT/tasks/bl-be99.md          # notes.jsonl (no core home) folded into the body
+++
title = "Auth backend"
...
+++
Token issuance service.

## Notes (migrated)

- 2026-05-01T09:05:00Z alice: Chose JWT over opaque tokens.
- 2026-05-01T09:10:00Z bob: Rotate signing keys weekly.
```

`bl-ch01.md` carries `parent = "bl-ep01"` and a `[[blockers]] on = "claim"` for
`bl-be99` (its `depends_on`). `id`/`status`/`branch`/`closed_children`/`external`
are all dropped (derived or plugin territory, §3/§16).

## 6. Hole 2 — the live cutover proves the epic stays blocked

The RUNBOOK: `bl prime` founds the substrate (config needs *no* migration — the
seed **is** the migrated config, every legacy knob dissolved), then the script
writes the migrated tasks straight into the founded store:

```console
$ bl prime --as carol                # founds balls/config landing + empty balls/tasks store
$ STORE=$(ls -d "$XDG_STATE_HOME"/balls/clones/*/tasks)
$ python3 migrate-legacy.py --repo $LEGACY --into "$STORE"
legacy: 6 tasks (5 live) → greenfield: 5 migrated
  ...
wrote 5 migrated tasks → …/tasks/tasks
$ git -C "$STORE" add -A && git -C "$STORE" commit -qm "balls: migrate tasks"
$ bl prime --as carol                # re-converges; idempotent
```

The migrated store loads through the **real binary** — and the epic is
`blocked`, not spuriously `ready`:

```console
$ bl list
ready    bl-be99  Auth backend  p1
blocked  bl-ep01  Auth epic  p1
blocked  bl-ch01  Wire the login child  p2
ready    bl-ch02  Session cookies  p2
ready    bl-orph  Revisit the spike  p3

$ bl dep-tree
ready    bl-be99  Auth backend
blocked  bl-ep01  Auth epic [needs bl-ch01, needs bl-ch02]
  blocked  bl-ch01  Wire the login child [needs bl-be99]
  ready    bl-ch02  Session cookies
ready    bl-orph  Revisit the spike
```

The epic `[needs bl-ch01, needs bl-ch02]` is the minted reciprocal edge — legacy
derived "epic waits on its children" from status; greenfield `parent` is
containment-only (§10), so the migrator mints the `claim`-blocker explicitly.
`bl-ch01` `[needs bl-be99]` is the `depends_on` edge. `bl-dead` (closed) never
appears. `bl-orph` surfaces as `ready` despite `deferred` — **intended**, not a
regression (§16: deferred is a tag).

## 7. Hole 3 — `github-issues adopt` stamps the join marker

Legacy kept the join inline as `external.github-issues.issue.number`; the base
migrator **drops** it (plugin territory), so the issues are marker-less and a
naive first `sync` would auto-create a duplicate per issue. `adopt` amends the
join SSOT directly — it appends `[bl-id]` to each mirrored issue's title (one
PATCH). It runs **online**; here against a mock GitHub serving the issue API.

```console
$ python3 titles.py                  # issue titles BEFORE adopt — all marker-less
  #101  open     Wire the login form  (marker-less)
  #102  open     Session cookies  (marker-less)
  #103  open     Auth backend service  (marker-less)
  #104  open     Revisit the spike  (marker-less)
  #199  closed   Spike that shipped  (marker-less)

$ balls-plugin-github-issues adopt $LEGACY_TASKS $CONFIG_JSON
github-issues: stamped 4 legacy issue marker(s); skipped 2
```

`stamped 4` = the live mirrored issues; `skipped 2` = `bl-dead` (closed, issue
199) and `bl-ep01` (never mirrored). The server's audit shows exactly four
PATCHes:

```console
$ cat adopt-audit.log
PATCH issue #104: 'Revisit the spike' -> 'Revisit the spike [bl-orph]'
PATCH issue #102: 'Session cookies' -> 'Session cookies [bl-ch02]'
PATCH issue #101: 'Wire the login form' -> 'Wire the login form [bl-ch01]'
PATCH issue #103: 'Auth backend service' -> 'Auth backend service [bl-be99]'

$ python3 titles.py                  # AFTER — the join marker is now on GitHub
  #101  open     Wire the login form [bl-ch01]
  #102  open     Session cookies [bl-ch02]
  #103  open     Auth backend service [bl-be99]
  #104  open     Revisit the spike [bl-orph]
  #199  closed   Spike that shipped  (marker-less)
```

**Idempotent** — a re-run reads each title and PATCHes nothing already correct:

```console
$ : > adopt-audit.log
$ balls-plugin-github-issues adopt $LEGACY_TASKS $CONFIG_JSON
github-issues: stamped 4 legacy issue marker(s); skipped 2
$ cat adopt-audit.log
  (no PATCH calls — re-run was a no-op)
```

## 8. The federation caveat — a clone that skips `adopt` still joins

The marker lives on GitHub, so the join needs **no per-machine state**. A second
federated clone with no territory, no base, and that never ran `adopt` recovers
the link from the title alone (faithful to `marker::strip` — a trailing
`[bl-…]`), exactly as `sync`'s pull does (priority 1):

```console
$ python3 clone2_join.py             # clone-2: reads only GitHub, no local adopt state
  #101 JOIN  -> bl-ch01   (recovered from the marker, no local state)
  #102 JOIN  -> bl-ch02   (recovered from the marker, no local state)
  #103 JOIN  -> bl-be99   (recovered from the marker, no local state)
  #104 JOIN  -> bl-orph   (recovered from the marker, no local state)
  #199 DUP   -> (no marker; sync would auto-create a duplicate)
```

So the **per-clone adopt caveat is dissolved** by the bl-0ef9 rework: once *any*
box runs `adopt`, every clone joins via the on-GitHub marker — there is no
per-machine join store to lose. `#199` (the one issue `adopt` deliberately left
marker-less — its task is closed, so there is no live ball to dup against anyway)
shows what a marker-less issue does: it is the shape of the **bounded floor**.
Skip `adopt` entirely on all machines and *every* issue looks like `#199` — the
first `sync` auto-creates one duplicate per task, then re-records and stabilizes.
Never a runaway; the accepted cost of migrate-clean-or-delink (§16).

---

## What this proves

- **Hole 1 (branch collision)** — the migrator writes a markdown `balls/tasks`
  beside the legacy JSON `balls/tasks`, plus a fresh sibling `balls/config`;
  the cutover keeps the legacy history (§4 — as demoed, a `balls-archive`
  branch before a force-push; since bl-8660, in-branch via the runbook's
  history join).
- **Hole 2 (epic edges)** — a migrated epic mints a reciprocal `claim`-blocker
  per live child and loads `blocked` through the real `bl`, not spuriously
  `ready` (§10/§16).
- **Hole 3 (gh dup)** — `adopt` stamps the `[bl-xxxx]` title marker (one PATCH
  per live mirrored issue, idempotent), so the first `sync` joins instead of
  duplicating (§16).
- **Hole 4 (claimed-guard)** — the one-shot script refuses to migrate over a
  `claimed` task; quiesce-first is the contract, not converge (§16).
- **Federation caveat** — the marker-on-GitHub SSOT makes a skip-`adopt` clone
  join with zero dup; only a total skip costs one bounded dup per task (bl-0ef9).

**Reproduce:** build `bl`/`tracker`/`bl-delivery` (`cargo build --release`) and
the github-issues plugin from `main`; put them on `PATH`; run the steps above in
a throwaway repo with an isolated `XDG_STATE_HOME`. The migration script is
`scripts/migrate-legacy.py`; its self-test (`--self-test`) needs no repo at all.
