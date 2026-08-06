# bl list — list tasks

    usage: bl list [NEEDLE] [-s ready|blocked|claimed|closed] [--all] [--everywhere]
             [--tag T] [--claimant NAME] [--since YYYY-MM-DD] [--until YYYY-MM-DD]
             [--json] [--plain] [--legacy]

The single listing verb. Default = live (non-closed) tasks, highest priority
first, SCOPED to this checkout's project, rendered as a CONTAINMENT TREE (see
below). Filters COMPOSE (AND).

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
    bl list retry --all                  # RECALL: needle over title+body, live AND closed
                                         #   — "has this been tried before?"

## The human view is a containment tree

Rows nest under their `--parent`, two spaces per level:

    ready    bl-epic  Ship the delivery gate  p2
      claimed  bl-9a1c  Wire the hook  p1  @alice (3h)  ->bl-epic
        ready    bl-77b2  Update the docs
    ready    bl-4d10  Unrelated ball  p1

The tree is a FOREST OVER WHAT IS RENDERED, and that is the whole rule: a row
indents under its parent only when that parent is in the same listing. A parent
that is closed, filtered out (`-s ready` drops a claimed epic), or foreign to
this checkout leaves its child rendering flush left, in its own place — no
special case, no dangling-parent error.

Ordering is unchanged, it just applies PER LEVEL: priority ascending (absent
last), then `created`, among siblings. So a low-priority parent does pull its
high-priority children down the page with it — deliberate, since a child is
unreadable out of its parent's context (that is the point: five identically
titled `Update the docs` gates are only distinguishable by who owns them).

Derived and human-only, like claim-age, the fleet label and the `->` marker:
there is no `--tree`/`--flat` flag and no stored field. `--json` is untouched —
a FLAT array in the global order, carrying `parent` for a machine to shape
itself.

## Claim-age is a derived, human-only column

A claimed row hangs its holder's claim-age off the `@claimant` (`@alice (3h)`,
coarse minutes/hours/days from the claim commit's timestamp), so `bl list -s
claimed` doubles as the fleet's staleness view. It is DERIVED and human-only:
`--json` carries stored frontmatter alone (no age), so a machine reader derives
it itself. There is no `--count` — count a rung with `bl list -s ready | wc -l`.

## The delivery-target column: `->bl-xxxx`

A row ending `  ->bl-epic` means this ball's work does **not** go to the
integration branch — it forks from and folds back into `work/bl-epic`, because
the ball both sits under `bl-epic` (`--parent`) and close-gates it (nesting
needs BOTH coordinates; a bare `--parent` is containment only and stays flat).
No marker = the integration branch, the default and the overwhelming case.

On a **closed** row the marker is the "delivered, not landed" signal: the ball
is done and squashed onto its target ref, but nothing reached main yet. Its
absence on a closed row means landed — a target derives only against a LIVE
target ball, so the marker vanishes the moment that ball closes and lands. To
find where work actually is, follow the chain: each target renders its own
target, up to the parentless ball whose target is the integration branch.

Derived and human-only, like claim-age and the fleet label: `--json` carries
stored frontmatter alone. There is no `target` field to filter on and none to
store — it is `parent` plus a `close` blocker, both already in the record.

The marker is NOT the tree, and both showing on one row is not redundancy: the
indent says where the ball LIVES (containment, `--parent` alone), the marker
says where its work GOES (routing, `--parent` AND a close-gate). A contained
child with no gate indents and carries no marker.

## Status is derived, never stored

A task has no `status` field. The three live states are **computed on read**:

- **claimed** — someone holds it (the `claimant` field is set).
- **blocked** — unclaimed, but an unresolved `claim`-blocker remains.
- **ready** — unclaimed with every `claim`-blocker resolved; claimable now.

`-s closed` (or `--all` for live + dead) reconstructs archived tasks from
history — a closed task has no file, so absence is what "resolved" means. The
usual session read is `bl list` (or `bl list -s ready`) to pick work, and `bl
list -s claimed` to resume your own.
