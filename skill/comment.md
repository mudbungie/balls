# bl comment — append a note to a task's body

    usage: bl comment <id> "TEXT" [--as ID] [--remote URL]

Appends TEXT to the task's markdown body under a horizontal rule, and seals it
exactly as `bl update` seals a `--body` rewrite. It is sugar over `update`,
nothing more: read the stored body, append, seal.

    $ bl comment bl-1a2b "single-clone runs pass; needs a second clone"
    $ bl show bl-1a2b

    the original body, as filed

    ---

    single-clone runs pass; needs a second clone

Each comment brings its own rule, so the seams are visible in the render. The
rule is **decoration for the reader and nothing more** — balls writes it once and
never reads it back: it never searches for a rule, counts them, or splits the
body on one. Appending to an EMPTY body emits no rule; a rule separates two
things, and there is nothing above the first comment to separate it from.

## Flags

- `--as ID` — worker identity.
- `--remote URL` — per-op store remote (see `bl prime --skill`).
- `-C PATH` — **global** (every command): address the store keyed by PATH, as
  if `bl` had run there. No walking, no git-root discovery.

## Examples

    bl comment bl-1a2b "repro needs a second clone; single-clone runs pass"
    bl comment bl-1a2b "$(cat notes.md)"

## Why the body, not the journal

`-m` on any verb writes the **journal** — the store branch's git history, which
`bl show <id>` renders as a `journal` section. That history is DERIVED, so the
bedrock `--json` export cannot carry it: `bl show` and `bl show --json` disagree
about what the ball says, and an agent reading the machine projection never sees
the note.

The body is **stored** state. `--json` carries `body` (the bedrock record is
total, so `bl show --json | bl import` round-trips the whole ball), and the human
render prints it. So appending to the body is the one place a note lands in
**both** views with no new mechanism — and that is the entire justification for
this verb. If it did not render in both, it would not be worth building.

Three ways to write, one distinction:

- `bl update <id> --body B` — overwrite the living document (current state).
- `bl update <id> -m MSG` — write a journal entry (history, human view only).
- `bl comment <id> "TEXT"` — append to the living document (both views).

## No stamp, no attribution, no marker

Under the rule the append is the literal text. Nothing else: no timestamp, no
`@who`, no id. The commit records who and when authoritatively — a stamp in the
body would be a second copy of a fact git already owns, and a copy that can drift
(hand-edited through `bl update --edit`, re-imported through `bl import`).

Consequences, all intended:

- The body stays **opaque markdown**. balls parses none of it — the rule included
  — so `--edit` cannot corrupt a structure and `--body` still overwrites
  wholesale: a comment IS body, and rewriting the body rewrites every comment and
  every rule with it.
- Two concurrent comments conflict textually like any two writes to one file; the
  ordinary seal handles it exactly as it handles any concurrent body write.

## Refusals

- Empty or whitespace-only TEXT is **refused**. A no-op append would seal
  nothing, and a note that silently vanishes is the failure the `-m` no-op abort
  already exists to prevent (bl-cf93).
- There is **no `-m`**: echoing the text into the commit note would store one
  fact twice. The commit subject is the ball's title (as on every verb) and the
  diff already shows the text.
- There is **no stdin flag** — `bl comment bl-1a2b "$(cat notes.md)"` already
  works.
- Field flags (`--title`/`--body`/`-t`/…) and `--edit` are refused: the append is
  the whole payload. Reach for `bl update` when you mean to overwrite.

`comment` is an ordinary op, so `comment.pre`/`comment.post` are hook keys like
any other (`bl conf --skill`) and `--needs X:comment` is expressible.
