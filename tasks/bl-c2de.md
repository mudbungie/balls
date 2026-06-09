+++
title = "bl conf — local config read/write (scalars + plugin-schedule lists); unify the store-remote ladder to one per-op tier"
created = 1781039831
updated = 1781039831
priority = 2
tags = ["design"]
+++
## Why

Surfaced by the bl-d234 federation demo: my first run primed a satellite with
`bl prime --remote $HUB`, then `create` on the center + `bl sync` on the
satellite **silently failed to converge** — both resolved to stealth and never
touched the hub. Root cause is a model mismatch with two parts:

1. **Two remote-resolution ladders, and the documented one isn't the one that
   governs convergence.** `prime` resolves `--remote > --center > XDG > origin`
   (`checkout.rs::prime`→`bind`); but `create`/`claim`/`close` resolve `XDG >
   origin` only (`mutate.rs:64` hardcodes `xdg_remote`), and `sync` passes
   `cli_remote=None` (`checkout.rs:129`). `--remote`/`--center` are **prime-only
   and persist NOTHING** — they shape one invocation's binding and die. So
   founding/joining via `--remote` leaves no durable pointer; the next op
   re-derives `XDG > origin`, finds neither, and goes stealth. The failure is
   invisible because no-remote is a *legitimate* mode (stealth) — the system
   can't tell "deliberately stealth" from "meant to federate, origin/XDG unset".

2. **`SKILL.md:65` actively teaches the wrong model:** *"To point a fresh
   checkout at a shared project, pass the remote once: `bl prime --remote <url>`
   … Re-running plain `bl prime` later converges to a no-op."* "Pass once" + "plain
   prime later is a no-op" promises the remote is remembered. It isn't — that
   sentence is only true when `--remote` equals `origin`, i.e. when the flag was
   redundant. The task author had the same assumption, evidence it's the natural
   reading.

And the deeper gap underneath: **there is no config-set surface at all** (the 12
verbs have no `conf`). Config changes only by hand-editing TOML — under
percent-encoded XDG clone dirs nobody can find — or by `install`. §4/§12 sanction
local edits ("config changes **by you** or by install"), but give "by you" no
ergonomic path, and no way to even *see* what remote/branch a checkout resolves
or where its files are.

## The design: `bl conf` — the "by you" local config path (§4/§12)

A new verb for local config CRUD. It edits **only your local config** (landing
`balls.toml`/`plugins.toml`, XDG `config.toml`); it never crosses a checkout
boundary (that stays `install`'s consent-gated job) and never touches a binary
(the `bin/<name>` adjacency stays the unchanged RCE gate — naming a plugin whose
binary isn't symlinked is a pruned no-op at prime, exactly as today). Config never
syncs (§12), so `conf` is purely local: no store seal, no plugin dispatch.

**Read (provenance closes the gap that bit bl-d234):**
- `bl conf` — dump every resolved value + which layer it came from + the file
  paths (XDG config, landing config, store). This is the "where are my files /
  what remote am I actually using" answer; stealth shows as `task-remote: (none)`.
- `bl conf <key>` — print one resolved value + provenance.

**Write (symmetric, scope-keyed):**
- `bl conf set <key> <value>` — whole override: scalar replace, or bare-field
  full **list** replace.
- `bl conf append <key> <value>` / `bl conf prepend <key> <value>` — list compose,
  emitting the §4 `<field>_append` / `<field>_prepend` directives.
- `bl conf remove <key> <value>` — prune an entry by name → §4 `<field>_ban`.

**Keys + canonical writable home** (each key's home is where §4 *allows* that
field to live; reads resolve across all layers and report the tier):

| key | underlying field | type | `set`/list-op writes |
|---|---|---|---|
| `task-remote` | the store remote | scalar | **XDG** `config.toml` (per-machine; remote is NOT a landing field by design — a URL must not travel on `install`) |
| `task-branch` | `tasks_branch` | scalar | **landing** `balls.toml` (committed on `balls/config`) |
| `log-level` | `log_level` | scalar | **landing** `balls.toml` |
| `<op>.<phase>` (e.g. `close.pre`) | `plugins.toml` `[hooks]` | **list** | **landing** `plugins.toml` |

The list ops (`append`/`prepend`/`remove`) apply to the `[hooks]` schedule — the
only list config (§4). `set` on a scalar replaces; `set` on a hooks key
bare-replaces the whole list.

**Scope:** the key implies its home (no `--scope` flag) — `task-remote` is
per-machine (XDG, its only legal home), the rest are per-repo (landing). Per-repo
remote durability remains `git remote add origin <hub>`, git-native and read as
the bottom ladder tier; `conf` does not wrap `git remote` (clean provenance — bl
writes bl's files, git owns origin).

## Bundled (same misalignment — keep together unless this ball is split)

- **Uniform `--remote`/`--center` override on every op** (`create`/`claim`/
  `close`/`sync`), not just `prime`. Collapses the two ladders into ONE —
  `--remote`/`--center` (per-op override) > XDG > origin — resolved identically
  everywhere. Removes the prime-only special case (the missing reframe: every op
  resolves the remote the same way; prime isn't special). Today `mutate.rs:64`
  and `checkout.rs:129` block this.
- **`prime` warns** when it founds/joins on a remote the durable ladder (XDG >
  origin) won't reproduce: *"founded on `<hub>` via `--remote`; this checkout has
  no `origin`/XDG remote, so plain commands will be stealth — set `origin` or
  `task-remote` to federate."* This is §12's own *"non-default store, no install →
  a WARNING, not silence"* pattern; it would have caught the bl-d234 failure live.
- **Docs:** rewrite `SKILL.md:65` (drop "pass the remote once"; state the store
  remote is `origin`/XDG and `--remote`/`--center` are per-op overrides; name the
  remote in the `bl sync` table row); split §12's SEAM paragraph so it stops
  lumping `--remote`/`--center` with XDG as one "explicit tiers" set (prime had a
  4-tier ladder; with the unification every op shares one ladder).

## Invariants preserved (attack surface)

- `install` stays the ONLY cross-boundary config+code adoption (consent); `conf`
  is local-only and cannot fetch or activate a foreign config/binary.
- `bin/<name>` adjacency stays the RCE gate; `conf` writes the *schedule*, never a
  binary — an unbacked schedule entry is pruned at prime (harmless, unchanged).
- Landing single-owner / never-pushed (§4) holds: config never syncs, so a `conf`
  edit is purely local.
- `tasks_branch` re-home discipline unchanged: `conf set task-branch` carries the
  same repoint-strands-the-store caveat as the sanctioned hand-edit (move the
  store first, §12) — `conf` doesn't worsen it, and the provenance read makes a
  mispoint visible.

## Spec + build impact

- **Spec (frozen — this is a deliberate post-cutover amendment):** §9 (new verb),
  §4 (the "by you" path gets a surface; key namespace), §12 (one unified ladder +
  prime warn + wording).
- **Build:** `Verb::Conf`, `OpClass::Diffless` (authors no ball diff, no store
  seal); help directory auto-generates from `Verb::ALL`. Writes: XDG keys = plain
  file write; landing keys = edit + commit on `balls/config`. Likely its own
  module (`src/conf.rs` read + a write sibling) to stay under the 300-line cap.
- **Tests:** 100% coverage gate — unit-test each key × verb × error path in `src`
  (`tests/` is coverage-neutral).

## Open calls

- **Parent/scope:** filed standalone. Reparent under the impl epic, or split into
  sub-balls (conf verb / uniform override / warn+docs)?
- **Further subtraction:** does the XDG `task-remote` tier still earn its keep once
  `origin` is the per-repo durable and `--remote` is the per-op override? It may be
  cuttable (origin + per-op flag could cover it).