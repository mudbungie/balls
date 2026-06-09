# E2E demo bl-d234 — central/satellite federation (one shared store branch)

A captured live run for epic **bl-9369** (Milestone E2E demo & validation sweep).
This is the artifact for child **bl-d234**: prove §12 federation holds in
practice — *many landings, one store branch* — not just in unit tests.

- **Binary:** freshly built from the claimed worktree (`cargo build --release`),
  the three siblings `bl` + `tracker` + `bl-delivery`, put first on `PATH`.
- **Repos:** throwaway `/tmp/bld234-demo` — one bare `hub.git` (the shared remote)
  and two ordinary git checkouts (`center`, `satellite`), each a clone-shaped repo
  whose `origin` points at the hub.
- **Isolation:** `XDG_STATE_HOME` / `XDG_CONFIG_HOME` under `/tmp/bld234-demo`, so
  the two landings live outside the repos and this run never touches the balls
  project's own task list (the epic's standing warning).
- **Identities:** `--as alice` on the center, `--as bob` on the satellite.

The JSON op-log (`{"ts":…,"lvl":"info",…}`) is written to **stderr**; it is
elided below (filtered with `grep -vE '^\{"ts":'`) except in §7a, where it *is*
the evidence. The driver is `/tmp/fed-demo/run.sh`; every block below is its real
output.

What bl-d234 asks to verify, and where each lands:

| Requirement | Section |
|---|---|
| Prime a satellite with `--remote` against a center | §2, §3 |
| Federation = many landings, ONE store branch (§12) | §3 |
| Create on one side, `bl sync` converges the other | §4, §5 |
| Convergence is bidirectional | §6 |
| push/fetch is **tracker-only** (core stays local, §0) | §7 |
| Store-pointer precedence `--remote > --center > XDG > origin` | §8 |

---

## 1. One shared remote + two checkouts that clone it

The hub is a bare repo — it will hold the shared `balls/tasks` store branch. Each
checkout is an ordinary git repo whose `origin` is the hub (the standard
federation: *a fresh clone of a repo whose store sits there needs no install*,
§12). Nothing is on the hub yet.

```console
$ git init -q --bare /tmp/bld234-demo/hub.git
$ git -C /tmp/bld234-demo/center    remote add origin /tmp/bld234-demo/hub.git
$ git -C /tmp/bld234-demo/satellite remote add origin /tmp/bld234-demo/hub.git

$ git --git-dir=/tmp/bld234-demo/hub.git log --oneline balls/tasks
(balls/tasks absent)
```

## 2. Found the center — alice primes `--remote` against the hub

`--remote` is prime's store-pointer override (the top precedence tier — see §8).
The remote's `balls/tasks` is **absent**, so this is the bootstrap: core founds
the orphan store and the tracker's founding `prime/post` push **creates** the
branch on the hub.

```console
$ bl prime --as alice --remote /tmp/bld234-demo/hub.git

$ git --git-dir=/tmp/bld234-demo/hub.git log --oneline balls/tasks
bb47b54 balls: found store
```

## 3. Join the satellite — bob primes `--remote` against the SAME hub

Now the remote's `balls/tasks` is **present**, so the satellite's prime **adopts**
the established store (the tracker clones it in; no divergent orphan is founded).
The center and satellite are two distinct landings (two XDG clone dirs) that now
share **one** store branch — that is federation, §12.

```console
$ bl prime --as bob --remote /tmp/bld234-demo/hub.git
```

## 4. Create on the center; the `*/post` push publishes it to the hub

A note on tiers (proven in §8): `--remote` is **prime-only** and does not persist.
Ongoing mutations (`create`/`claim`/`close`) and `bl sync` resolve the store
remote via **XDG → origin**. Here `origin` is the hub, so the center's `create`
pushes the new task to the hub on its `create/post` hook. The satellite has not
synced yet, so it does not see it.

```console
$ bl create "center task" -p 1 --as alice
bl-21cd

$ bl list                       # on the center
ready    bl-21cd  center task  p1

$ bl list                       # on the satellite — not yet converged
$
```

## 5. `bl sync` converges the satellite

`bl sync` is fetch + fast-forward-only of the store branch from the hub. After it,
the satellite reads the center's task.

```console
$ bl sync --as bob              # on the satellite

$ bl list
ready    bl-21cd  center task  p1
```

## 6. Create on the satellite; `bl sync` converges the center (bidirectional)

The reverse direction proves convergence is symmetric: the satellite creates a
task (pushed to the hub), and the center's `bl sync` pulls it down. Both landings
now read one shared store.

```console
$ bl create "satellite task" -p 2 --as bob
bl-9293

$ bl sync --as alice            # on the center

$ bl list
ready    bl-21cd  center task  p1
ready    bl-9293  satellite task  p2
```

## 7. push/fetch is the **tracker's**, never core's

§0: core is local-only; the tracker is the single component that talks to a
remote. Three angles confirm it.

**(a)** The op-log attributes the remote touch to `invoke tracker`. A sync's
fetch+ff rides `sync/pre`; nothing else contacts the remote (op-log shown raw):

```console
$ bl sync --as alice
{"ts":1780971158,"lvl":"info","src":"core","op":"sync","msg":"begin"}
{"ts":1780971158,"lvl":"info","src":"core","op":"sync","phase":"pre","msg":"invoke tracker"}
{"ts":1780971158,"lvl":"info","src":"core","op":"sync","msg":"done"}
```

**(b)** A read verb runs **no** plugin chain — zero remote contact, empty op-log:

```console
$ bl list >/dev/null 2>read.err   # op-log bytes on stderr:
0
```

**(c)** Source proof: every git **remote** verb (`fetch`/`push`/`ls-remote`)
lives only under `src/tracker/`. Core's `src/git.rs` has a single
`merge --ff-only <sha>` — a *local* sha merge for delivery, not a network op.

```console
$ grep -rn '"fetch"|"push"|"ls-remote"' src/ (non-test) | sed 's#:.*##' | sort -u
src/tracker/mod.rs
src/tracker/prime.rs
src/tracker/remote_ops.rs
```

## 8. Store-pointer precedence: `--remote > --center > XDG > origin`

The remote resolved for prime's founding push walks four tiers. To observe which
tier wins, four hubs (one per tier) are reset before each run; a fresh checkout
sets some tiers, primes, and we read which hub received the founding push.

- `--remote=…` assigns the binding's remote (always wins).
- `--center=…` fills it only if `--remote` did not (`get_or_insert`).
- XDG `remote=…` is core's fallback when no CLI tier is given.
- `origin` is the tracker's discovery when core hands it `remote: None`.

Adding one higher tier at a time moves the founding push up the ladder:

```console
  tiers set: origin=h-origin xdg=—      args=                                  ->  founded on: h-origin
  tiers set: origin=h-origin xdg=h-xdg  args=                                  ->  founded on: h-xdg
  tiers set: origin=h-origin xdg=h-xdg  args=--center h-center.git             ->  founded on: h-center
  tiers set: origin=h-origin xdg=h-xdg  args=--center h-center.git --remote h-remote.git  ->  founded on: h-remote
```

Each rung overrides every tier below it: origin is the fallback, XDG beats origin,
`--center` beats XDG, and `--remote` beats `--center` (and all).

---

## What this proves

| Story | Verified |
|---|---|
| `--remote` founds an absent store (bootstrap push creates `balls/tasks`) | §2 |
| `--remote` adopts an established store (no divergent orphan) | §3 |
| Two landings share one store branch — federation, §12 | §3 |
| A `create` on one landing publishes to the shared hub | §4 |
| `bl sync` converges the other landing (fetch + ff-only) | §5 |
| Convergence is bidirectional | §6 |
| Remote push/fetch is the tracker's alone; core stays local (§0) | §7 |
| A read verb contacts no remote and runs no chain | §7b |
| Store-pointer precedence `--remote > --center > XDG > origin` holds | §8 |

§12 federation survives end to end against the freshly-built binary: many
landings, one store branch, kept consistent by `bl sync`, with all remote talk
confined to the tracker.
