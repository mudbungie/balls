# bl install — copy committed config/plugins between branches

    usage: bl install [PATH] [--from REF] [--to REF] [--bin NAME=PATH] [--as ID]

Copies a committed PATH between branches, sealed as one commit on `--to`'s tip
(capability transfer). **Shape decides the merge:** a folder mirrors (deletions
propagate!), a file/glob is an additive union; `bin/` never travels.

## Flags

- `PATH` — committed path to copy (default: `config`).
- `--from REF` — source branch (default: the configured upstream, fetched by the
  `install.pre` tracker).
- `--to REF` — target branch (default: the landing `balls/config`).
- `--bin NAME=PATH` — bind a plugin binary (a landing-targeted install only).
- `--as ID` — worker identity.

## Examples

    bl install config

## Landing installs bind their plugins

A landing-targeted install, after the sealed copy, binds each plugin the landed
schedule references — beside `bl`, then on `$PATH`, `--bin NAME=PATH` overriding
per plugin — validated against its live `protocol`. A refusal lands AFTER the
sealed copy (the commit is the undo; the retry converges and just binds). Prints
`N added / M deleted`. A referenced name with no binary anywhere stays dangling
and is reported, one line each (`referenced but not bound … re-run bl install
after acquiring`); re-running after acquiring converges and just binds.

## Where a missing binary comes from: the `[source]` hint

`plugins.toml` may carry a `[source]` table — per-name free text the schedule's
owner authors (`bl-adversary = "cargo install balls-adversary"`). balls displays
it verbatim at the refusals (the dispatch unbound abort, the validation refusal,
the dangling report, the seed prune) and never parses, fetches, or runs it:
acquiring the binary is your explicit act, via your package manager. No hint ⇒
the same refusals, terser.

## Repairing a dangling plugin in install's own chain

Binding runs after the copy seals, so a schedule wiring a **not-yet-bound**
plugin into `install.pre`/`install.post` aborts every retry at dispatch, before
`--bin` can act. `bl conf` runs no plugins, so the escape is in-band: `bl conf
remove install.pre <name>`, then `bl install --bin <name>=<path>` to bind, then
`bl conf prepend install.pre <name>`. If that entry was the plugin's ONLY
reference, `--bin` is refused (unreferenced names never bind silently) — park a
temporary reference on a read hook first (`bl conf append list <name>`, harmless:
a failed read dispatch is non-fatal) and drop it after the bind.

See `bl conf --skill` for the plugin schedule this install binds against.
