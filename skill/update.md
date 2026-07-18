# bl update — overwrite any field of a task

    usage: bl update <id> [--edit] [--title T] [--body B]
             [--parent ID|--no-parent] [-p N|--no-priority]
             [-t TAG|--no-tag TAG] [--needs ID[:OP]|--no-needs ID]
             [key=value] [-m MSG] [--as ID]

Overwrites **any** field of a task. There is no create-only split except
reciprocal `--blocks` (an edge on ANOTHER task; that stays create-only).

## Flags

- `--edit` — open the stored task in `$EDITOR` (human-only; excludes the field
  flags).
- `--title T` — retitle.
- `--body B` — rewrite the markdown body.
- `--parent ID` / `--no-parent` — set or clear the parent pointer.
- `-p N` / `--no-priority` — set or clear priority.
- `-t TAG` / `--no-tag TAG` — add or drop a tag.
- `--needs ID[:OP]` / `--no-needs ID` — add or unlink one of THIS task's own
  blockers (the in-band fix for a mis-wired blocker). An add that would close a
  claim/close cycle — e.g. `--needs X` on a task that already close-gates X —
  is refused, naming the loop (bl-54fe); the unlink always passes. See "No
  cycles through claim/close" in `bl create --skill`.
- `key=value` — set a preserved extra field (a bare `key=` removes it).
- `-m MSG` — commit note. A zero-edit update appends a progress note.
- `--as ID` — worker identity.

## Examples

    bl update bl-1a2b --body "now waiting on the upstream release"
    bl update bl-1a2b -m "progress note (rides git history, not the body)"

## Body vs journal: `--body` vs `-m`

`--body` is the task's **living document** (current state — overwrite it when the
state changes). `-m` is the **append-only journal entry**, riding the update
commit's message on the store branch. There is no `comment` verb and no
body-append flag — the journal IS git history (`git log -- tasks/<id>.md` in the
store checkout), and human `bl show <id>` renders it as a `journal` section after
the body, oldest-first, one entry per store commit. Taking over a ball, read the
prior agent's notes there. `--json` stays the bedrock frontmatter mirror and
never carries the journal (it is derived history).

A pure-note update always commits (the `updated` restamp); if truly nothing
changed — a second write inside the same wall-clock second — the op **fails**
rather than drop the note. Retry a second later.

## `--edit` (the human projection)

`--edit` opens the stored `tasks/<id>.md` (frontmatter + body) in `$EDITOR` (else
`$VISUAL`), blocking, then runs the saved buffer through the normal update seal.
Mutually exclusive with the field flags and `key=value` extras (they would race
over the payload). A non-tty stdin or an unset editor is an **error**, so agents
keep using flag-driven update. The buffer is parse-validated on save (bad TOML /
a missing required field is rejected). The id is the path and `created` is
history, so neither is hand-editable; `updated` is always restamped by the seal.
