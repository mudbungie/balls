# bl list — list tasks

    usage: bl list [NEEDLE] [-s ready|blocked|claimed|closed] [--all] [--everywhere]
             [--tag T] [--claimant NAME] [--since YYYY-MM-DD] [--until YYYY-MM-DD]
             [--json] [--plain] [--legacy]

The single listing verb. Default = live (non-closed) tasks, highest priority
first, SCOPED to this checkout's project. Filters COMPOSE (AND).

## Flags

- `NEEDLE` — one positional: a case-insensitive substring over the title AND body
  (`bl list delivery`).
- `-s, --status RUNG` — filter to one status: `ready` | `blocked` | `claimed` |
  `closed`.
- `--all` — include closed tasks (live + dead).
- `--everywhere` — show every project on the store, not just this checkout's.
  One store can serve many projects; the default set is what `claim` admits here
  (this checkout's git root + rootless balls), so foreign projects' balls are
  hidden. `--everywhere` lifts that scope and labels each foreign row with a short
  project name (an enrolled checkout's basename, else the root's short hash —
  human render only, never in `--json`). `show` is always global.
- `--tag T` — filter by tag (repeatable, AND).
- `--claimant NAME` — filter to tasks held by NAME (exact match); pairs with `-s
  closed` to answer "what did NAME deliver".
- `--since YYYY-MM-DD` — tasks updated on or after the date.
- `--until YYYY-MM-DD` — tasks updated on or before the date.
- `--json` — lossless machine records (stored frontmatter only; **no** derived
  claim-age). Parse these, not the tty view.
- `--plain` — no color or status glyphs.
- `--legacy[=REF]` — preview a legacy store's live set.

## Examples

    bl list                # everything live, priority-ordered
    bl list -s ready       # claimable now — the dispatch set (this project)
    bl list -s claimed     # tasks someone already owns (resume these)
    bl list delivery       # NEEDLE: title/body substring
    bl list --everywhere -s ready        # every project on the store — fleet dispatch
    bl list -s closed --claimant alice   # what alice delivered

## Claim-age is a derived, human-only column

A claimed row hangs its holder's claim-age off the `@claimant` (`@alice (3h)`,
coarse minutes/hours/days from the claim commit's timestamp), so `bl list -s
claimed` doubles as the fleet's staleness view. It is DERIVED and human-only:
`--json` carries stored frontmatter alone (no age), so a machine reader derives
it itself. There is no `--count` — count a rung with `bl list -s ready | wc -l`.

## Status is derived, never stored

A task has no `status` field. The three live states are **computed on read**:

- **claimed** — someone holds it (the `claimant` field is set).
- **blocked** — unclaimed, but an unresolved `claim`-blocker remains.
- **ready** — unclaimed with every `claim`-blocker resolved; claimable now.

`-s closed` (or `--all` for live + dead) reconstructs archived tasks from
history — a closed task has no file, so absence is what "resolved" means. The
usual session read is `bl list` (or `bl list -s ready`) to pick work, and `bl
list -s claimed` to resume your own.
