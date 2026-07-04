# bl import — bulk-create tasks from JSON on stdin

    usage: bl import [--legacy[=REF]] [-m MSG] [--as ID]

Ingests verbatim task JSON (the `bl show --json` / `bl list --json` bedrock
shape) from **stdin**. Ids and timestamps are preserved — no mint, no stamp, no
gate. One commit, all-or-nothing: an id that already exists refuses the whole
stream (use `bl update` to modify an existing task).

`import` is the write inverse of the bedrock read, and a distinct primitive from
`create`: "reproduce an existing identity" (migration, restore, federation join)
is not "mint a new one", which is exactly why `create` refuses foreign ids.

## Flags

- `--legacy[=REF]` — instead migrate a pre-greenfield store (preview first with
  `bl list --legacy`).
- `-m MSG` — commit note.
- `--as ID` — worker identity.

## Examples

    cat new-tasks.json | bl import        # create new tasks (won't overwrite existing ids)
    bl import --legacy                    # migrate an old store
