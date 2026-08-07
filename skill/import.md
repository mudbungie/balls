# bl import — bulk-create tasks from JSON on stdin

    usage: bl import [--legacy[=REF]] [-m MSG] [--as ID]

Ingests verbatim task JSON (the `bl show --json` / `bl list --json` bedrock
shape) from **stdin**. Ids and timestamps are preserved — no mint, no stamp, no
gate. One commit, all-or-nothing: an id that already exists refuses the whole
stream (use `bl update` to modify an existing task).

`import` is the write inverse of the bedrock read, and a distinct primitive from
`create`: "reproduce an existing identity" (migration, restore, federation join)
is not "mint a new one", which is exactly why `create` refuses foreign ids.

## Reopening a closed ball

**This store's own history is a source like any other**, so the round trip
reopens a retired ball — there is no `reopen` verb because this is it:

    bl show bl-1a2b --json | bl import

`bl show` resolves a closed id out of `balls/tasks` history (a deletion is older
CONTENT, not a tombstone) and the bedrock record is TOTAL — frontmatter and body
— precisely so import can write it back. Nothing is undone: the close commit and
the deletion both stand, and what you get is a ball that exists again carrying
the id and content it had before. That is a reproduction, not a transition,
which is exactly what import is for.

Two things fall out of the ordinary rules rather than needing their own:

- **A live id refuses.** Ids of closed balls are legally re-minted, so `bl-1a2b`
  may today be an unrelated ball — the collision refusal catches it and imports
  nothing, naming the id.
- **The restored `claimant` is stale.** A ball closed while claimed comes back
  claimed, by an agent whose `work/<id>` worktree the close tore down. `bl
  unclaim <id>` clears it. (Or drop the key from the record before the pipe, if
  you would rather it never land.)

Restoring the ball says nothing about the CODE: `bl close` squashed `work/<id>`
onto the delivery target before archiving the task, and that commit stands.
Reverting it is an ordinary `git revert`, a separate and deliberate act.

Verbatim stops at **shape**. Every id in a record — its own, its `parent`, and
each blocker's — must be a safe path token (`^[A-Za-z0-9][A-Za-z0-9_-]*$`),
because an id IS a filename and the edges are read back as `tasks/<id>.md`; one
that is not refuses the stream, naming the field. Liveness is NOT checked: an
edge pointing at a ball this store has never seen imports fine, which is what a
peer or a filtered stream carries.

Verbatim includes the **edges**: blockers arrive exactly as the stream spells
them, with no create-time sugar applied. So a stream carrying an older store's
subtasks — `{child, claim}` edges on the epic, which is what `--subtask-of` used
to mint — reproduces claim-gated epics that keep delivering flat to the
integration branch. Nothing converts them; nesting is declared by a `close` edge
(`bl create --skill`), so to nest an imported tree, spell the `close` edges in
the stream. An import authors no delivery of its own — no worktree, no target.

Verbatim also includes `root_commit` — the project identity a ball recorded at
birth. Import a ball rooted in ANOTHER project and it lands correctly but sits
outside this checkout's default `bl list` scope, which shows only what `bl claim`
would admit. The import says so: alongside the count it prints one stderr hint
naming how many records are rooted elsewhere and the read that lifts the scope,
`bl list --everywhere`. The ball is there — `bl show <id>` resolves it — it is
simply not this project's.

## Flags

- `--legacy[=REF]` — instead migrate a pre-greenfield store (preview first with
  `bl list --legacy`).
- `-m MSG` — commit note.
- `--as ID` — worker identity.

## Examples

    cat new-tasks.json | bl import        # create new tasks (won't overwrite existing ids)
    bl show bl-1a2b --json | bl import    # reopen a closed ball
    bl import --legacy                    # migrate an old store
