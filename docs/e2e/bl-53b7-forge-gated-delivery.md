# E2E demo bl-53b7 — forge-gated delivery (close.pre approval gate)

A captured live run for epic **bl-9369** (Milestone E2E demo & validation sweep).
This is the artifact for child **bl-53b7**: prove that an opt-in *submit/approve*
split — the forge variant of delivery — holds a task's delivery behind an
approval gate, then squashes and tears down exactly as the solo flow does once
the gate clears.

- **Binary:** freshly built from `main` (`cargo build --release`), the siblings
  `bl` + `bl-delivery`, put first on `PATH`. `tracker` is deliberately **not**
  installed beside them, so the seed prunes it — this run is remote-less.
- **Repo:** a throwaway `/tmp/bl53b7-demo/acme` — `git init` + one empty commit,
  nothing more.
- **Isolation:** `XDG_STATE_HOME=/tmp/bl53b7-demo/xdgstate` (+ an isolated `HOME`
  / `XDG_CONFIG_HOME`), so this run never touches the balls project's own task
  list (per the epic's standing warning).
- **Identities:** `--as dev` is the author who claims and delivers; `--as lead`
  is the reviewer who approves. The split is the whole point of this scenario.

## How the gate works (no gating plugin)

The default `bl` flow is solo: the agent that claims also closes, and
`close.pre`'s `bl-delivery` squashes the work to `main`. The *forge variant* adds
an external approval hold **without a new lifecycle node and without a gating
plugin**. Per `src/enforce.rs`:

> "There is no gating plugin: forge/build gates **open gate children and rely on
> those children BLOCKING**, so the enforcer cannot be an optional install. …
> for `close` before any `close.pre` plugin (e.g. delivery) squashes."

So an approval gate is just a **gate child**: a task whose *close* blocks the
parent's *close* (a `{id, on: close}` blocker edge — the §10 op-keyed guard). The
gate child stands in for the forge's pre-merge review unit (a GitHub PR, a GitLab
MR…). A forge plugin's `sync` would close it on PR-merge (gating model:
docs/architecture.md §10); here the reviewer closes it by hand. Core refuses the parent's `bl close`
— **before** `close.pre` delivery ever runs — until that child is gone.

The JSON op-log (`{"ts":…,"lvl":"info",…}`) is written to **stderr**; it is
elided below (filtered with `grep -vE '^\{"ts":'`) so the terse stderr
confirmations and real stdout remain. Read verbs use `2>/dev/null` directly. Each
mutating verb's real exit status is printed as `exit=N`.

What bl-53b7 asks to verify, and where each lands below:

| Requirement | Section |
|---|---|
| the gate **refuses** an un-approved close, **naming the gate** | §4 |
| the close **permits** after approval | §5, §6 |
| delivery **still squashes correctly** (one `[bl-xxxx]` commit) | §7a, §7b |
| **teardown** runs (worktree removed) | §7c |

---

```console
===== 0. Setup — an ordinary git repo (no bl init, no .balls/) =====

$ command -v bl
/tmp/bl

$ git init -q -b main && git commit -q --allow-empty -m "Initial commit"
$ git log --oneline
c450931 Initial commit
```

```console
===== 1. Found the substrate — bl prime (remote-less: tracker is not installed) =====

$ bl prime --as dev
exit=0

===== 1b. The seeded close.pre wiring — bl-delivery IS the close.pre hook =====

$ sed -n '/\[hooks\]/,$p' "$(find "$XDG_STATE_HOME"/balls -name plugins.toml)"
[hooks]
"claim.post" = ["bl-delivery"]
"claim.pre" = ["bl-delivery"]
"close.post" = ["bl-delivery"]
"close.pre" = ["bl-delivery"]
"drop.post" = ["bl-delivery"]
"prime.post" = ["bl-delivery"]
"unclaim.post" = ["bl-delivery"]
"unclaim.pre" = ["bl-delivery"]
```

`close.pre = ["bl-delivery"]` is the delivery squash. The seed **pruned** every
`tracker` entry because no `tracker` binary sits beside `bl` (remote-less box).
The forge gate is *not* in this table — it is core enforcement that fires in
front of `close.pre`, so it needs no plugin and no config of its own.

```console
===== 2. The author claims the work task and does the work IN the worktree =====

$ WORK=$(bl create "Add /metrics endpoint" -p 1 -t backend --as dev 2>/dev/null); echo "$WORK"
bl-d39a

$ bl claim "$WORK" --as dev
claim bl-d39a
exit=0

$ WT=$(git worktree list | awk '/work\/bl-d39a/{print $1}'); echo "$WT"
/tmp/bl53b7-demo/xdgstate/balls/plugins/bl-delivery/tmp/bl53b7-demo/acme/bl-d39a

$ ( cd "$WT" && printf 'pub fn metrics()->&'\''static str{"ok"}\n' > metrics.rs \
       && git add metrics.rs && git commit -qm 'metrics endpoint' && git log --oneline )
acfdcab metrics endpoint
c450931 Initial commit
```

All edits go on `work/<id>` in the worktree — this is the change a PR would
carry.

```console
===== 3. OPEN THE FORGE GATE — a gate child whose close blocks the parent's close =====

$ GATE=$(bl create "Forge: PR approved for $WORK" -t forge-gate \
           --parent "$WORK" --blocks close --as dev 2>/dev/null); echo "$GATE"
bl-308d
```

`--parent WORK --blocks close` is the §10/§15 front door for the retired
`--gates`: it mints a child whose **close** gates the parent's **close**. The
parent now carries a `{id, on: close}` blocker:

```console
$ bl dep-tree 2>/dev/null
claimed  bl-d39a  Add /metrics endpoint [gate bl-308d]
  ready    bl-308d  Forge: PR approved for bl-d39a

$ bl show "$WORK" --json 2>/dev/null | grep -A3 blockers
  "blockers": [
    {
      "id": "bl-308d",
      "on": "close"

$ bl list 2>/dev/null
claimed  bl-d39a  Add /metrics endpoint  p1  @dev
ready    bl-308d  Forge: PR approved for bl-d39a
```

The parent is claimed and its code is ready, but its delivery is now held by an
open gate child — the "PR opened, awaiting approval" state.

```console
===== 4. The gate REFUSES the un-approved close — and names the gate =====

$ bl close "$WORK" -m 'deliver metrics' --as dev
bl close: authoring the base change failed: close: bl-d39a blocked by unresolved bl-308d
exit=1

===== 4b. Nothing was mutated — main untouched, worktree intact =====

$ git log --oneline main
c450931 Initial commit

$ git worktree list | awk '/work\/bl-d39a/{print "intact: "$1}'
intact: /tmp/bl53b7-demo/xdgstate/balls/plugins/bl-delivery/tmp/bl53b7-demo/acme/bl-d39a
```

The refusal **names the unresolved gate** (`blocked by unresolved bl-308d`) and
exits non-zero. It fires in core *before* the `close.pre` delivery hook, so the
squash never ran: `main` still has only the seed commit and the work worktree is
untouched.

```console
===== 5. APPROVE — the reviewer closes the forge gate child (PR merged) =====

$ bl close "$GATE" -m 'PR approved & merged' --as lead
close bl-308d
exit=0

$ bl dep-tree 2>/dev/null
claimed  bl-d39a  Add /metrics endpoint [gate bl-308d]

$ bl list 2>/dev/null
claimed  bl-d39a  Add /metrics endpoint  p1  @dev
```

A *different* identity (`lead`) closes the gate child — the approval. The gate
child's row is gone from the live tree (its `tasks/bl-308d.md` is deleted), so
the blocker resolves by file-absence. The stored `gates` edge is single-source-
of-truth and is **not** deleted, so `dep-tree` still renders the `[gate bl-308d]`
annotation — but it now points at a closed task, and the parent reads plain
`claimed`, no longer blocked.

```console
===== 6. Close is now PERMITTED — delivery squashes + teardown, in one move =====

$ bl close "$WORK" -m 'deliver metrics' --as dev
close bl-d39a
exit=0
```

Same command that was refused in §4 now succeeds: with the gate child gone, core
lets `close` through to `close.pre` delivery.

```console
===== 7a. main carries ONE [bl-xxxx] delivery commit (squash still correct) =====

$ git log --oneline main
8122bc1 Add /metrics endpoint [bl-d39a]
c450931 Initial commit

===== 7b. the worktree file is on main =====

$ git show main:metrics.rs
pub fn metrics()->&'static str{"ok"}

===== 7c. teardown ran — the worktree is removed =====

$ git worktree list | awk '/work\/bl-d39a/{print $1}' | grep . || echo "(worktree removed)"
(worktree removed)

===== 7d. both tasks closed; live set empty =====

$ bl list 2>/dev/null; echo "(live set empty, exit $?)"
(live set empty, exit 0)

$ bl list -s closed 2>/dev/null
closed   bl-d39a  Add /metrics endpoint  p1  @dev
closed   bl-308d  Forge: PR approved for bl-d39a
```

The whole `work/bl-d39a` branch collapsed to a **single** `[bl-d39a]`-tagged
commit on `main` — byte-for-byte the same delivery shape as the solo flow
(bl-7911 §7a) — its file is on `main`, and `close.post` tore the worktree down.
The gate changed *when* delivery was allowed, not *how* it landed.

---

## What this proves

| Guarantee (bl-53b7) | Verified |
|---|---|
| An approval gate is a gate child (`on: close` blocker), no new status, no gating plugin | §3, §1b |
| The gate **refuses** an un-approved `close`, **naming** the unresolved gate, exit 1 | §4 |
| The refusal fires before `close.pre`: `main` untouched, worktree intact | §4b |
| Approval = a (different identity's) close of the gate child; the parent then reads closeable | §5 |
| Once approved, `close` **squashes** to `main` as one `[bl-xxxx]` commit — identical to the solo flow | §6, §7a, §7b |
| `close.post` **teardown** removes the `work/<id>` worktree | §7c |
| Both parent and gate child end **closed**; the live set is empty | §7d |

The forge variant of delivery holds end to end against the freshly-built binary
in a throwaway repo: the gate gates, approval clears it, and delivery + teardown
proceed unchanged.
