# bl conf — read or write this checkout's local config

    usage: bl conf [<key>]
           bl conf set|append|prepend|remove <key> <value...>

Reads and writes this checkout's local config. **Local-only**: `conf` never
crosses a checkout boundary (adopting another checkout's config is `install`'s
consent-gated job — see `bl install --skill`) and runs no plugins. Config never
syncs.

## Reading

- `bl conf` (no args) — dump every resolved value, the **layer** it came from
  (`cli`/`binding`/`xdg`/`landing`/`origin`/`default`), and the paths of the
  files behind them.
- `bl conf <key>` — print one value (stdout) with its provenance (stderr). A
  checkout with no durable remote shows `task-remote (none)` — that checkout is
  stealth.

## Writing (scope-keyed — the key implies the file, there is no `--scope`)

- `bl conf set task-remote <url>` — per-checkout store remote (this clone's
  binding; also clears a declared stealth sentinel).
- `bl conf set task-remote none` — declare **stealth**: a landing-committed
  per-checkout sentinel (what `bl prime --stealth` sugars to).
- `bl conf set task-branch <name>` / `bl conf set log-level <level>` — landing
  `balls.toml`, committed on `balls/config`. Re-pointing `task-branch` strands
  the store unless you move it first.
- `bl conf set|append|prepend|remove <op>.<pre|post> <name...>` — the `[hooks]`
  plugin schedule. `show`/`list` are bare keys (`bl conf append list <name>`).

## Examples

    bl conf
    bl conf set task-remote git@host:repo.git
    bl conf append list myplugin

## The plugin schedule

Behavior beyond the base (commit task files) is **plugins** — subprocesses wired
under `[hooks]` (`<op>.<phase>` → an ordered list of plugin names). Two ship
wired by default (`bl-tracker` = the remote; `bl-delivery` = the `work/<id>`
worktree); a third, `bl-chore`, ships but is opt-in.

`set` replaces the whole list; `append`/`prepend`/`remove` compose one name and
converge (a present name re-appended, or an absent one removed, is a no-op).
Naming a plugin whose binary isn't installed beside `bl` leaves a dangling entry
— pruned at seed, a clean error at dispatch — never code execution; `conf` writes
the schedule, never a binary.

**Order is yours, and it matters.** Plugins run in list order; on abort, whatever
ran rolls back in reverse. Nothing enforces the seeded order. When wiring your
own plugin, **prepend** to post phases (`bl conf prepend <op>.post <name>`) or
`conf set` the full order; only the irreversible belongs last (tracker pushes,
delivery squashes). The natural gesture `conf append <op>.post` lands your plugin
AFTER tracker — if it fails there, the un-seal resets a commit the remote already
has and the next push is rejected non-fast-forward (recoverable: `bl sync` then
retry, but surprising).

`bl-chore` (opt-in) mints one close-gate child per configured chore at
`claim.post` — a forcing-function checklist, not CI. Opt in with `bl conf prepend
claim.post bl-chore`, then write `config/plugins/bl-chore/chores.toml`. Binding a
plugin's binary between branches is `bl install --skill`.
