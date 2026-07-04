# bl-8ab5 — query surface: packaged derived reads, no index

**CONVERGED (2026-07-04).** Drafted by Paint 2026-07-02; converged by
maintainer dialogue 2026-07-04. The maintainer's convergence statement
(verbatim): *"yes, I think we are converged, we want the queries. The design
principle here is that a user _can_ break into the tasks branch and start
poking around, but they shouldn't have to to do their basic job. A user having
to bust open the tasks branch probably implies a failure of the ergonomic
surface."*

That principle is the test this design answers to, and the sharp edge of the
three-layer rule below: a basic-job question (the hourly fleet queries of Q1)
must have a one-command answer on the surface — needing `git log` against the
store checkout for one of those IS an ergonomics bug, file it as one. The
store stays open for archaeology and for machine consumers (the reaper's
one-liner, jq over `--json` bedrock — `--json` is surface, not break-in); those
paths are sanctioned, not failures.

The maintainer's framing question (verbatim, 2026-07-01): *"I tend to think that
index building is a giant complication. Technically everything is files, and a
couple of clever greps and the like could deliver it, if the data is structured
right. Is this just packaging command surface to deliver those queries?"*

Answer: **yes — and half the packaging already shipped.** The data is already
structured right (one file per live ball, TOML frontmatter, §5 `bl-op` commit
trailers for every time-of-event fact, one-row-per-ball human render). The ball
body overstates the gap: text search, tag, and date filters landed 2026-06-07
(bl-206c, unified under `-s` by bl-7218). What is genuinely missing is **one
predicate** (`--claimant`), **one derived column** (claim-age), and
**discoverability** of what exists. Everything else on the ball's list — counts,
sort control, a generic predicate surface — is subtracted below: Unix and jq
already deliver it, and core would only re-implement it worse.

## Ground truth — the surface today (verified against main, 2026-07-02)

`bl list [-s ready|blocked|claimed|closed] [--all] [--tag T]… [--since D]
[--until D] [NEEDLE] [--json] [--plain]` — filters compose AND, applied
uniformly to the live and (when reached) history-reconstructed dead set
(`src/reads/filter.rs`, `src/reads/flags.rs`):

- **`NEEDLE`** (positional) — case-insensitive substring over title AND body.
  *"Which balls mention the delivery plugin?" has had a one-command answer
  (`bl list delivery`) since bl-206c* — but `bl help list` does not mention the
  positional, so agents re-derive it with jq every session. The gap is
  discoverability, not capability.
- **`-s`** — the one status axis; `closed` infers the dead-set reach.
- **`--tag`** — repeatable AND-subset. **`--since`/`--until`** — day-bounded
  window over `created`/effective-`updated`.
- Order is fixed: priority ascending (absent last), created, id (§10) —
  display-only, uniform over live and dead.
- `--json` is bedrock (stored frontmatter only, §3 bl-d074); the human render
  freely paints derived columns (badge, `pN`, `@claimant`).

## Q1 — the queries a fleet runs hourly, and their disposition

| fleet question | today | disposition |
|---|---|---|
| what's ready for me | `bl list -s ready` | shipped |
| what is agent X holding | `bl list -s claimed \| grep '@X$'` | **add `--claimant X`** |
| what's been claimed longest / anything stale | no answer | **add claim-age column** |
| which balls mention *topic* | `bl list <needle>` | shipped; **fix help + skill text** |
| everything tagged T | `bl list --tag T` | shipped |
| what closed this week | `bl list -s closed --since …` | shipped |
| what did X deliver | none (needs claimant axis) | falls out of `--claimant` — reconstruction keeps the pre-deletion frontmatter, and 172 of this store's 184 dead balls retain `claimant` |
| counts by rung | `bl list -s R \| wc -l` | jq/wc is fine — see subtractions |
| arbitrary composition | `--json \| jq` | sanctioned bedrock path — document the idioms |

## Q2 / Q4 — the shape rule: where flags stop and jq begins

Three layers, each with a hard boundary. This is the anti-proliferation answer:
the flag count is bounded by the schema, so the surface has a **completion
point** rather than a growth curve.

1. **Predicates (list flags) read stored frontmatter alone.** That is already
   `filter.rs`'s documented contract; keep it an invariant. The §3 schema bounds
   the axes: status, tags, date-window, text, claimant — and then it is
   COMPLETE. (`parent` is `show`'s tree; `priority` is the order, not a filter;
   `blockers` surface as the status ladder.) A filter over a *derived* fact is
   refused on principle — `--stale-over 2h` would smuggle a policy threshold
   into core; staleness policy belongs to the liveness plugin (bl-1e98).
2. **Derived facts are human-render columns, never bedrock fields.** The §3
   bl-d074 split already licenses this: the human projection freely paints
   derived columns; `--json` stays the lossless mirror of stored frontmatter.
   Machine consumers derive from the store directly — the §11 worktree-path
   precedent (human `show` folds the worktree line in; `--json` never carries
   it; `git worktree list` is the machine read), and bl-0e16 (journal in the
   human `show`, bedrock unchanged) is the same shape in flight.
3. **Composition is jq over bedrock.** A generic predicate surface (`--where`,
   a query mini-language) re-implements jq worse, inside core, forever. Refused.
   The skill guide should carry the three or four blessed one-liners instead.

**No new verb** (Q4): every addition here is a `list` flag, a human-render
column, or help/skill text. `list` stays the single listing verb.

## Proposal 1 — `--claimant NAME`

One more compose-AND predicate over a stored field, closing the last schema
axis. Uniform over live and dead rows like every filter, which is what makes
`bl list -s closed --claimant X` ("what did X deliver") fall out for free.
Exact-match on the stored string (claimants are `--as` identities, not prose).

Subtraction attack, answered: `grep '@X$'` works today because `@claimant` is
the row's last column — but it is a coincidence of render order, breaks the
moment a column lands after it (the claim-age column below does exactly that),
and has no dead-set analogue. The flag is the stable spelling; the grep was the
workaround.

## Proposal 2 — claim-age, derived at render

**Claim time already exists in the store; nothing new is stored.** It is the
timestamp of the newest commit touching `tasks/<id>.md` whose §5 trailer is
`bl-op: claim` — newest-wins makes an unclaim/reclaim cycle resolve to the
*current* claim, the same recency discipline as every §9 history read. Verified
against this store:

```
$ git log -1 --format=%ct --grep='^bl-op: claim$' -- tasks/bl-8ab5.md
1782968775        # = this ball's claim instant, to the second
```

- **Surface:** an age column on claimed rows in the human `list` render
  (`claimed bl-8ab5 …  p1  @Paint  3h`) and a `claimed <ISO> (<age>)` line in
  human `show`. `bl list -s claimed` thereby becomes the fleet's staleness
  dashboard. Bedrock `--json` is untouched.
- **Cost:** one `git log` walk per *claimed* row (~10 ms here), and the claimed
  set is small by nature (it is bounded by fleet size, not store size). Live
  rows only — dead rows render retirement, not claim-age.
- **Machine path:** a reaper/dispatcher runs the same one-liner against the
  store checkout (`$XDG_STATE_HOME/balls/clones/…/tasks`). Deliberately NOT a
  second JSON projection: two machine contracts would drift; the store is the
  machine API for time-of-event facts, exactly as the journal is `git log`
  (§9 bl-cf93) and the worktree path is `git worktree list` (§11).

## Subtractions — what this design refuses

- **No `--count`.** The human render's one-row-per-ball is a contract worth
  stating in the skill guide, and then `bl list -s ready | wc -l` IS the count.
  Counts-by-status composes from four short commands or one
  `--json | jq 'group_by(…)'`; neither is an hourly loop's inner step.
- **No `--sort`.** Priority is "the one ordering input" (§3); §10's order is
  THE order. "Longest-claimed first" is `sort` on the age column or jq;
  a sort flag is a display preference riding core forever.
- **No `--where` / query language.** Layer 3 above.
- **No new verb.** Q4; `list` already owns listing.
- **No cache, no index.** Q3 below — the measured price does not justify one.

## Q3 — the closed-set price, measured

This store (608 commits, 184 dead balls), 2026-07-02: the full-history
enumeration walk (`git log --diff-filter=D --name-only -- tasks`) costs
**33 ms** — the O(history) part is cheap and stays cheap. The observed 2.27 s
for `bl list -s closed` is the **per-ball reconstruction**: two subprocesses
(`git log -1` + `git show`) × 184 ≈ 12 ms/ball. Extrapolated, a 10 000-close
store answers a dead-set query in ~2 minutes.

**Accepted price — because frequency and cost are inversely matched.** The
hourly fleet queries (ready, claimed, claimant, age) touch live files plus a
claimed-set-sized walk; the dead set is archaeology, minutes-tolerant. And the
remedy, should it ever hurt, is a *smarter derivation, never a stored one*:
batch reconstruction through a single `git log --diff-filter=D -p` walk (one
subprocess instead of 2 N), and push `--since`/`--until` into the enumeration
(`git log --since`) so a bounded question does bounded work. The no-index rule
survives contact with the numbers.

## Q5 — what falls out for bl-1e98 (claim liveness)

1. **The claim-time derivation is settled and verified here** — the one-liner
   above is bl-1e98's foundation; that ball must not re-open "derive vs store".
2. **Its surface pre-exists:** the age column makes `bl list -s claimed` the
   staleness dashboard; liveness adds policy, not query surface.
3. **The threshold stays out of core by layer rule 1:** filters read stored
   fields only, so "stale after 2 h" can never be a `list` flag; it is reaper
   plugin config, and the reaper derives age by the same store-checkout
   one-liner (machine path above).
4. Cooperative occupancy (no identity check on unclaim) is untouched by this
   ball — bl-1e98's question, informed but not constrained.

## Convergence (2026-07-04) — the open questions, settled

1. **`--claimant` is in.** The maintainer wants the queries; the
   axis-completeness rule ("flags = schema axes, complete at claimant") holds
   as the anti-creep boundary. Under the convergence principle, `grep '@X$'`
   was a break-in-shaped workaround for a basic-job question — exactly the
   ergonomic failure the surface must absorb.
2. **Age renders attached to the claimant: `@Paint (3h)`** — age is a fact
   about the claim, not a free-floating column. `show`'s line:
   `claimed 2026-07-02T05:06:15Z (3h ago)`. (Settled by default lean, not
   maintainer fiat — a render tweak at implementation is cheap if it reads
   badly in practice.)
3. **The discoverability fix rides the implementation ball** — help/skill text
   documenting the needle, the blessed jq idioms, and the one-row contract is
   part of delivering the surface, not a separate docs ball. Under the
   principle, an undocumented capability and a missing one fail the same test.

Implementation notes for the follow-up ball: a partial spike (unverified
`--claimant` + claim-age work in src/reads) is preserved as tag
`bl-8ab5-spike` (stash-shaped commit; `git stash apply bl-8ab5-spike`) — it
predates the bl-7858 decomposition refactor, so treat it as reference, not a
base. Claim-time derivation one-liner verified in Proposal 2 above.
