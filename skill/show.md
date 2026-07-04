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
