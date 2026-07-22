# bl show — show one task in full

    usage: bl show <id> [--json] [--plain] [--legacy[=REF]]

Prints one task in full: fields, blockers, children, body, and journal (the
ball's store history with its `-m` notes, oldest-first). A closed id still
resolves (reconstructed from history).

## Flags

- `--json` — the **lossless machine record**: raw stored frontmatter, literal
  integer timestamps, no derived fields. This is the bedrock; `bl import` ingests
  the same shape back. **Agents should always parse `--json`, never the tty
  view.**
- `--plain` — no color or status glyphs (the human view without a tty).
- `--legacy[=REF]` — project one ball from a legacy store.

## Examples

    bl show bl-1a2b
    bl show bl-1a2b --json

## Notes

The human view folds in a `worktree` line when the `work/<id>` worktree exists on
this machine (a computed, machine-local field), and — for a live, currently-
claimed ball — a derived `claimed <ISO> (<age> ago)` line under the `claimant`
field. Both are human-only and store-derived: `--json` carries neither, nor the
journal (derived history). See `bl update --skill` for how the journal is written
(`-m`) and `bl list --skill` for how status and claim-age are derived.

A nested ball also gets a `delivers <id>` line under `parent`: its work forks
from and folds back into `work/<id>`, not the integration branch. It appears
only when the ball BOTH sits under that parent and close-gates it — bare
containment stays flat. On a **closed** ball it reads "delivered there, not
landed on main"; its absence reads "landed", because the line derives only
against a live target. Same derived, human-only column `bl list` renders as
`->bl-xxxx` — see `bl list --skill`.

Every `bl show <id>` of a live ball also mints a local **seen-token** (`--json`
included): proof the current content reached your stdout, which is what lets a
later `bl close` of a ball someone else edited pass without a refusal — see
"Close refuses a task file you haven't seen" in `bl close --skill`. Tokens are
local acknowledgment cursors, never state: losing one costs at most one
refusal-with-diff, and stray ones are inert (`bl prime` sweeps dead ones).
