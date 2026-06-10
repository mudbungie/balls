# bl import — the write inverse of `show --json` (bl-e614)

Design record for dissolving the §16 migration script into verbs. Amends
architecture.md §16; the living spec text there is authoritative — this file
holds the decision reasoning at full length.

## The reframe

`scripts/migrate-legacy.py` wrote legacy state into the greenfield store
**below the verb layer**, and every smell it carried followed from that one
placement: a hand-rolled duplicate of the `task.rs` serializer (SSOT
violation), orphan-ref git plumbing to land branches without the substrate,
and id/timestamp gymnastics to preserve what the verbs refuse. The fix is not
a better script — it is giving the verb layer the one primitive it was
missing, then expressing migration as composition:

- **`bl list --legacy[=REF]` / `bl show --legacy`** — a bounded read shim:
  point the read verbs at the pre-greenfield `.balls/tasks/*.json` (default
  `balls/tasks:.balls/tasks`) and project it into the greenfield wire shape.
  ONE place owns "how to read legacy" (`src/reads/legacy.rs`); `bl list
  --legacy` *is* the migration preview, dissolving the old `--dry-run`.
- **`bl import`** — the write primitive: verbatim, fully-identified bedrock
  task JSON (the exact `show --json` / `list --json` shape) on stdin, written
  through the real store and the real serializer. Id and timestamps verbatim;
  no mint, no stamp, no gate.
- **`bl import --legacy[=REF]`** — the cutover button: exactly
  `bl list --legacy --json | bl import` in-process, plus the epic
  reciprocal-edge pass wired through ordinary `update --needs` ops. Pure
  orchestration — it carries no policy the pipe can't express, which is what
  stops it from becoming a second code path for one job.

The bedrock record became **total** for this: `--json` now carries `body`
alongside the frontmatter, because a record `import` writes back must be the
whole ball. Lossless was always the §9 contract; totality is its completion.

## Decisions

### Naming — `import` (decided in the task; recorded here)

It pairs with a future `export` and with the `show --json` read-duality
(adopt/export would be incoherent); `adopt` wrongly connotes ownership next to
`claim`; and `adopt.rs` already means config/policy adoption on prime — a
genuinely different operation (config vs task records), so distinct names
preserve a real distinction rather than falsely unifying two things.

### Import is not `create --id` — the trap, answered

`import` punches exactly the hole `create` refuses: a caller-supplied id and
historical timestamps. That refusal is correct *for create* — minting a NEW
identity is core's job (id = path, the engine stamps `now`), and a `--id` flag
would invite collisions and clock lies into the everyday path. `import` is a
different primitive: **"reproduce a task that already has an identity."** The
id is authoritative at the source — a federation peer, a backup, the legacy
store — and re-minting it would *break* the identity the operation exists to
carry. One refuses foreign identity because it mints; the other takes it
verbatim because it reproduces. Folding them into one verb with a flag would
make each the other's edge case.

`import` is deliberately NOT migration-specific: it is the shared primitive
behind federation join, restore-from-backup, and clone-seeding. Migration is
just its first caller.

### Collision semantics — refuse the whole stream, name the ids (THE open decision)

Importing an id that already lives in the store (or repeated within one
stream) **aborts the entire stream before anything is written**: exit nonzero,
the error naming every colliding id. Nothing imports; the store is untouched.

Why not the alternatives:

- **Skip** lies. The verb's contract is "reproduce these identities"; a
  restore that silently skipped a held id reports success while the store
  still carries the wrong ball. Worse, skip makes the outcome depend on local
  state the caller didn't name — the same import means different things on
  different machines. A lying restore is worse than a failed one.
- **Replace** destroys local state on no explicit signal. The colliding ball
  may be claimed, may carry local edits; clobbering it from a pipe is
  uninspectable data loss. Replace is also unrecoverable *as a default*:
  refuse can always be upgraded by the caller, replace cannot be undone by
  them.
- **A `--skip`/`--replace`/`--force` flag** is policy core can't justify
  carrying. Both remedies are already expressible with the verbs that exist:
  retire the holder (`bl close`) or strip the record from the stream (filter
  the JSON) and re-run. New flags are a smell (§0); the caller composes.

Why **whole-stream** rather than per-record: the import seals as ONE store
commit (all-or-nothing, the same §14 rollback guarantee every op has), so a
partial import would need either a partial seal or a "which half landed"
report — both more mechanism than the remedy deserves. A failed import
imports nothing; the error names what to fix; re-running is cheap and
idempotent-by-refusal. This is the store-side twin of §13's non-ff rule: a
collision IS the contention signal, surfaced, never auto-resolved.

Federation note: this default makes `import` safe to point at a live store —
the worst outcome of a bad stream is a named refusal. A future bulk
reconciliation ("take theirs where newer") is a *caller's* loop over
`show --json` + compare + close/import, not a core mode.

### `--legacy` scope — severable by construction

The legacy field-map (`claimed_by`→`claimant`, `depends_on`→blockers
`{id, on: claim}`, `type: epic`→tag, `status: deferred`→tag, ISO→epoch,
description+notes→body, closed-skipped, dangling-parent nulling) lives
read-side in `src/reads/legacy.rs`, behind the flag, and NOWHERE else — the
import button consumes the same projection the preview renders. Deleting the
capability post-cutover deletes: the module, the flag arms in the read parser
and import's parser, and the `Flags.legacy` field — no core edit
(footprint-demarcation: a bounded, documented exception, not a permanent
dialect). `--legacy` rejects the dead-set reach (`--all`/`-s closed`): the
legacy store has no greenfield history to reconstruct, and closed legacy tasks
deliberately do not migrate (closed = file-absent, §9).

### Epic edge — reconstruction rides the ordinary machinery

Legacy derived "an epic waits on its children" from status; greenfield
`parent:` is containment-only (§10). The shim is a RENAME, never a
reconstruction — so the cross-task reciprocal edge is minted by the button as
ordinary `bl update <parent> --needs <child>` ops (one per parent), reusing
the real edge logic, real seals, real `updated` stamps. No second transform
path; the projection stays pure and unit-testable.

## What died

The 412-line script + its python3 test dependency (`tests/migrate.rs`); the
duplicate serializer (import writes through `task.rs`); the ref plumbing
(`prime` founds, `import` fills, the tracker pushes); config rewriting (the
§12 seed IS the migrated config); the id/timestamp gymnastics (verbatim is the
contract); the bespoke epic-edge pass (ordinary verbs); `--dry-run`
(`bl list --legacy`). Remainder: the operator runbook —
`docs/migration-runbook.md` — where the "quiesce first / refuse if claimed"
guard lives as a runbook line, not code.
