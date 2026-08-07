# Comment attribution — the byline is a query, not a field (bl-236c)

Design record for the §9 comment byline. Amends `docs/architecture.md` §9
(`show` render, and the `comment` paragraph's pointer to it); the living spec
text there is authoritative — this file holds the decision reasoning at full
length. Implemented in `src/reads/attribution.rs`, sibling of
`src/reads/journal.rs`.

## The gap and the reframe

`bl comment` (bl-d136) appends TEXT to the body and stamps **nothing** into it:
no timestamp, no `@who`, no id. That is right and stays — the store commit
records who and when authoritatively, and a copy in the body drifts the moment
someone runs `--edit` or re-imports the ball.

What it leaves open is **co-location**. A body with six comments reads as one
undifferentiated document; answering "whose is this?" means `git log -p` in
another window. The gap is real, and the wrong fix is obvious: write the stamp
after all. That trades a live fact for a stale copy.

The reframe is the §0 rule verbatim — *don't store what you can compute, make it
a query.* The attribution is already in the store, exactly once, in the only
place that cannot drift: the commit. So derive it at render time and store
nothing. **Not a field, not a marker, not an index — a query.**

## The mechanism: the commit boundary IS the comment boundary

An append is one commit, so its lines are its lines. That single observation is
the whole design, and it is what makes marker parsing unnecessary:

1. `git blame --porcelain <rev> -- tasks/<id>.md` — the per-line commit map.
2. One `git log --no-walk` over the DISTINCT blamed commits, reading `%ct` and
   the §5 `bl-op` / `bl-actor` trailers (`%(trailers:key=…,valueonly=true)` — the
   read-side half of the no-hand-rolled-parser discipline).
3. Group the body's lines by commit; for each group whose op is `comment`, emit
   ONE added render line.

Cost: one blame plus one log per human `show`, and only for a non-empty body —
the journal walk's cost shape. Bedrock `--json` pays neither.

### The `---` rule is still never read

bl-d136 says balls writes the rule once and never reads it back. This does not
weaken that by a byte. Nothing here searches for a rule, counts rules, splits on
one, or suppresses one, and **no body byte is inspected at all**: the only thing
read off the body is its LINE COUNT, used to align git's per-line answer with the
tail of the file (frontmatter, fence, then body — so the body's lines are the
last *n*, with no fence parsing and no offset arithmetic). Body bytes are copied
through verbatim, newline for newline. The byline is an ADDED line.

## The one deviation: the byline hangs at the tail, not the head

The ball specified "an ADDED render line at the head of each comment region."
Implementation found that head placement misattributes on sight, so this record
amends it (per AGENTS.md: never implement a deviation silently — amend the doc).

`appended_body` writes `{existing}\n\n---\n\n{text}\n`, so a comment commit's
lines are, always and in this order: a blank line, the rule, a blank line, then
the text. Verified against real blame output. Therefore:

- The **head** of every comment region is decoration. A byline there lands
  directly under the PREVIOUS comment's last line of text and ABOVE the rule that
  opens its own — it reads as the previous comment's signature. Inserting a
  blank line above it does not fix this; it just leaves the byline floating
  between two comments, still above its own rule.
- The **tail** of every comment region is always the comment's own last line of
  text (the append ends `{text.trim_end()}\n`). A byline there is adjacent to
  exactly the text it attributes, and never touches the rule.

Tail placement is therefore the one that survives the no-reading-the-rule
constraint. It is not a taste call about signatures-versus-headers: head
placement is *wrong output* given where the decoration sits, and balls is not
allowed to look at the decoration to skip it.

## Human-only, on the boundary that already exists

`bl show --json` is unchanged, byte for byte, and does not pay the blame call.
This is not a special case — it is the journal's boundary applied again:

- Bedrock is the **round-trippable mirror of stored state**. `show --json |
  bl import` must reproduce the ball, and blame cannot survive that trip: import
  writes new commits under the importer, so every derived byline would collapse
  onto them. A bedrock field carrying attribution would be a lie one pipe later.
- Derived means human-only. The journal, the claim-age line, the `worktree`
  line, the `delivers` line and now the byline all sit on the same side.

A machine that wants attribution reads git: `git blame tasks/<id>.md` on the
store branch. That is where the fact lives, and it is not balls' to re-publish.

A `--legacy` read renders the body bare for the same reason it skips the journal:
that set's history lives on the legacy ref, not this store.

## Degradation is honest, never an error

The renderer states what blame says and repairs nothing. There is no detection
step, no warning, no fallback fabrication:

- **Imported ball** — one commit under `bl-op: import` owns every line, so
  nothing is a comment and nothing gets a byline. Correct: that IS who wrote that
  file, rules and comment text included.
- **Squashed or rewritten store** — collapses identically, for the same reason.
- **Closed ball** — blamed at the deletion's PARENT, the revision its
  reconstructed body actually came from. `Dead` now carries that revision
  (`Dead::rev`) rather than re-deriving it: the recency walk already computed it
  to read the content, and re-deriving would be a second representation of one
  fact. Blaming `HEAD` there would say nothing at all — the file is gone.
- **Empty body** — no blame call at all. Nothing to attribute, nothing paid.
- **A body git cannot blame** (written but never sealed, a store with no
  history) — the body renders bare. Blame is the ONE input; nothing said is
  nothing rendered. This is the same rule as every case above, not an error path,
  and it is why a half-written store still shows its balls.

## What this does not solve

- **Two comments in one commit.** They would render as one region with one
  byline. That cannot arise from `bl comment` (one append, one seal) and needs no
  guard; a hand-crafted commit gets an honest answer about itself.
- **Attribution for a `--body` rewrite that happens to be a note.** It renders
  bare, deliberately: the byline is a `comment`-op fact, and the op is the whole
  signal. Reach for `bl comment` when you mean a note.
- **Attribution in `--json`.** Refused on principle, above. Not a flag, not a
  future `--with-attribution` — the round trip is the argument.
