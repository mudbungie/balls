# balls — demo: plugin-less / remote-less degradation

A captured end-to-end run proving the **§6 guarantee**:

> *A plugin whose binary is not installed beside `bl` is pruned at prime, so a
> remote-less or plugin-less box still works.*

This is the bl-9a62 scenario of the bl-9369 validation sweep. Every block below is
real output from the **freshly-built greenfield binary** (`cargo build --release`)
driven against **throwaway `/tmp` git repos** with isolated `XDG_STATE_HOME` /
`XDG_CONFIG_HOME` — no network, no shared state, never this repo's own task list.

The JSON op-log lines (`{"ts":…,"src":"core",…}`) go to **stderr**. They are kept
here on purpose: their `…"msg":"invoke <plugin>"` records (or absence) are the
direct evidence of what the schedule dispatched.

The mechanism (src/seed.rs): at first `prime`, founding seeds the install-default
`plugins.toml`, binds each scheduled plugin to its sibling binary beside `bl`, and
then `hooks.retain(|name| present.contains(name))` **prunes every entry whose
binary did not resolve**. A box with only `bl` ends with an empty `[hooks]`; the
chain runs empty and core's base capability — commit the task file — carries the op.

---

## Two bin layouts

`bl` resolves a sibling plugin by adjacency — the binary must sit *beside* `bl`
(`Edge::resolve` looks in `bl`'s own directory). So the entire test is which
binaries share a directory:

```console
$ ls /tmp/dgrd-bin-only   # plugin-less box: only bl beside itself
bl
$ ls /tmp/dgrd-bin-full   # full box, for contrast
bl
bl-delivery
tracker
```

---

## Scenario A — plugin-less box (no `tracker`, no `bl-delivery` beside `bl`)

### Setup — an ordinary git repo, `bl` alone on PATH

```console
$ git init -q -b main && git commit -q --allow-empty -m 'Initial commit'
$ command -v bl
/tmp/dgrd-bin-only/bl
```

### Prime founds the substrate; both absent-binary plugins PRUNE

Note the op-log: **no `invoke` records at all** — nothing was dispatched, because
the schedule pruned to empty. (Contrast Scenario B, where prime logs
`invoke tracker` / `invoke bl-delivery`.)

```console
$ bl prime --as alice
{"ts":…,"src":"core","op":"prime","msg":"begin"}
{"ts":…,"src":"core","op":"prime","msg":"done"}
{"ts":…,"src":"core","op":"sync","msg":"begin"}
{"ts":…,"src":"core","op":"sync","msg":"done"}
```

The seeded landing schedule — every default entry pruned because no sibling
binary resolved:

```console
$ cat $XDG_STATE_HOME/balls/clones/<enc>/config/config/plugins.toml
[hooks]
```

### Core still commits task files — `create` (the base capability)

```console
$ BACKEND=$(bl create 'Wire auth backend' -p 1 -t backend --as alice); echo $BACKEND
{"ts":…,"src":"core","op":"create","msg":"begin"}
{"ts":…,"src":"core","op":"create","msg":"seal ddbfd04…"}
bl-f3ce
$ FORM=$(bl create 'Add login form' -p 2 --needs $BACKEND --as alice); echo $FORM
bl-5524
```

The op `seal`s a commit with no plugins in the chain — `tracker`'s `create.post`
push was pruned, but core wrote the file regardless.

### Every read verb works offline

```console
$ bl list
ready    bl-f3ce  Wire auth backend  p1
blocked  bl-5524  Add login form  p2          # blocked derivation still computes

$ bl show $BACKEND --json
{
  "blockers": [],
  "claimant": null,
  "created": 1780970815,
  "id": "bl-f3ce",
  "parent": null,
  "priority": 1,
  "tags": [ "backend" ],
  "title": "Wire auth backend",
  "updated": 1780970815
}
```

### Every mutate verb works offline and commits

`update`, `claim`, and `close` all succeed. With `bl-delivery` pruned, `claim`
sets the claimant and commits but materializes **no** `work/<id>` worktree, and
`close` archives the task without a squash-delivery — the degraded-but-correct
behavior. The base commit always lands.

```console
$ bl update $BACKEND -p 0 -m 'bump priority' --as alice
update bl-f3ce
$ bl claim $BACKEND --as alice
{"ts":…,"src":"core","op":"claim","msg":"seal 3d0c16c…"}
claim bl-f3ce
$ git worktree list | wc -l        # only the repo itself — no work/<id> materialized
1
$ bl list
claimed  bl-f3ce  Wire auth backend  p0  @alice
blocked  bl-5524  Add login form  p2
$ bl close $BACKEND --as alice
{"ts":…,"src":"core","op":"close","msg":"seal 3f70240…"}
close bl-f3ce
```

### The store is a real local commit history; nothing left the box

Every op — found, create, update, claim, close — is a commit core wrote to the
store branch, with no plugin in the chain and no remote anywhere:

```console
$ git -C $XDG_STATE_HOME/balls/clones/<enc>/tasks log --oneline
3f70240 Wire auth backend      # close
3d0c16c Wire auth backend      # claim
95d4b01 Wire auth backend      # update
533400f Add login form         # create
ddbfd04 Wire auth backend      # create
05b4ad4 balls: found store      # founding
$ git -C $XDG_STATE_HOME/balls/clones/<enc>/tasks remote -v
                                # (empty — no remote configured, fully offline)
$ bl list -s closed
closed   bl-f3ce  Wire auth backend  p0  @alice   # reconstructs from history
```

---

## Scenario B — remote-less box (all plugins PRESENT, but no remote)

The other half of the §6 guarantee: binaries are installed, so the schedule is
**retained**, but no remote is configured — `tracker` runs in **stealth** (store
stays local, a self-lock written; it never reaches out). The full lifecycle,
including worktree materialization and squash-delivery, works entirely offline.

### Prime retains the schedule (binaries resolve); tracker goes stealth

The op-log now *does* show `invoke tracker` / `invoke bl-delivery` — the contrast
with Scenario A:

```console
$ bl prime --as bob
{"ts":…,"src":"core","op":"prime","phase":"pre","msg":"invoke tracker"}
{"ts":…,"src":"core","op":"prime","phase":"post","msg":"invoke bl-delivery"}
{"ts":…,"src":"core","op":"prime","phase":"post","msg":"invoke tracker"}
{"ts":…,"src":"core","op":"sync","phase":"pre","msg":"invoke tracker"}

$ cat $XDG_STATE_HOME/balls/clones/<enc>/config/config/plugins.toml
[hooks]
"claim.post" = ["bl-delivery", "tracker"]
"claim.pre" = ["bl-delivery"]
"close.post" = ["bl-delivery", "tracker"]
"close.pre" = ["bl-delivery"]
"create.post" = ["tracker"]
"drop.post" = ["bl-delivery", "tracker"]
"install.pre" = ["tracker"]
"prime.post" = ["bl-delivery", "tracker"]
"prime.pre" = ["tracker"]
"sync.pre" = ["tracker"]
"unclaim.post" = ["bl-delivery", "tracker"]
"unclaim.pre" = ["bl-delivery"]
"update.post" = ["tracker"]
```

### Full lifecycle offline: create → claim → edit → close (squash-delivers)

```console
$ T=$(bl create 'Offline task' -p 1 --as bob); echo $T
bl-e830
$ bl claim $T --as bob
claim bl-e830
$ git worktree list                          # bl-delivery DID materialize the worktree
/tmp/dgrd-B/repo                                       9a490dc [main]
…/balls/plugins/bl-delivery/tmp/dgrd-B/repo/bl-e830    9a490dc [work/bl-e830]
$ echo 'offline edit' > .../bl-e830/FEATURE.txt && git -C .../bl-e830 commit -qam 'add feature'
$ bl close $T --as bob
close bl-e830
$ git -C /tmp/dgrd-B/repo log --oneline main   # squash-delivered locally, no remote
138d752 Offline task [bl-e830]
9a490dc Initial commit
$ git -C /tmp/dgrd-B/repo remote -v
                                # (empty — stealth tracker never pushed)
```

---

## What this proves

| Claim (§6) | Verified by |
|---|---|
| A plugin whose binary is absent beside `bl` is **pruned at prime** | Scenario A: seeded `[hooks]` is empty; op-log has no `invoke` records |
| Core still **commits task files** with zero plugins | Scenario A: store log shows found/create/update/claim/close commits |
| Every **read** verb works offline | Scenario A: `list`, `show --json`, `list -s closed` |
| Every **mutate** verb works offline | Scenario A: `create`, `update`, `claim`, `close` all `seal` a commit |
| Degraded `claim`/`close` are correct (no worktree, no delivery) when `bl-delivery` is pruned | Scenario A: `git worktree list` = 1; task archives without a `main` commit |
| A **remote-less** box with plugins present still works (tracker stealth) | Scenario B: full lifecycle + squash-delivery, `remote -v` empty throughout |

A remote-less *or* plugin-less box runs the full task lifecycle — the guarantee
holds against the shipped binary.
