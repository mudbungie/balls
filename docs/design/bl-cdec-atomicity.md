# bl-cdec — atomicity is a core guarantee

**OPEN (2026-07-25, Buildings).** Filed from bl-cdec, the lernie 0.0.1 release
drive (~15 concurrent agent closes, each gate a 10-13 min tarpaulin, load 25-30).
The report lists three failure modes. They are three symptoms of one absence:
balls has every component atomicity needs and has never stated the guarantee, so
each commit point was built by hand and two of them are wrong.

This document states the guarantee as **four obligations**, audits every op
against them, and lists the gaps. §6 is the part that is NOT settled and wants
the maintainer's attack.

## 1. What git actually does

Git's porcelain is segmented — hash the blobs, build the trees, write the commit,
move the ref — and it is atomic anyway, by four disciplines balls can copy
outright:

- **Inert until committed.** Everything before the ref move writes *objects*:
  immutable, content-addressed, unreachable. A crash leaves garbage nobody can
  observe and `gc` collects. There is no partially-applied state because the
  expensive work is not applied at all until one pointer moves.
- **The commit point is a compare-and-swap.** `update-ref <ref> <new> <old>`
  takes the **expected old value**; `push --atomic` and `update-ref --stdin`
  extend it to a multi-ref transaction. The old value is the one the new object
  was **derived from** — not a re-read at flip time. That is the whole of git's
  concurrency control.
- **Failure is a non-event.** `index.lock` is written and then `rename(2)`d over
  `index`; a failed operation leaves the original index byte-identical. Git does
  not unwind — there is nothing to unwind.
- **Carried state, not re-derivation.** Multi-step porcelain that must survive
  across processes writes what it needs down: `MERGE_HEAD`, `CHERRY_PICK_HEAD`,
  `.git/sequencer/`, `ORIG_HEAD`, and the reflog as the journal of every old
  value. `--continue`/`--abort` read those files; they never reconstruct intent
  from the working tree.

Note the absence: git has **no retry and no rollback**. A rejected CAS is
reported and the caller re-derives (`pull --rebase`). balls already adopted that
half — §14 converge-on-retry, the BINDING/NON-BINDING split — without adopting
the three disciplines that make it safe.

## 2. The guarantee

> **Every balls operation is atomic per repository.** It has exactly ONE commit
> point per repository it touches; that commit point is a compare-and-swap
> against the state the op's work was derived from; before it nothing is
> observable and after it everything is. An op spanning repositories is atomic in
> each and **convergent** across them: the commit points are ordered so that
> every prefix is a state a retry converges from.

Per-repository is the honest ceiling — git offers `--atomic` within one repo and
nothing across (bl-0161 already conceded this for cross-repo work). Convergence
is the glue, and §14 already specifies it. What was missing is the four
obligations that make each commit point actually atomic:

- **A1 — Prepare, then commit.** All fallible and expensive work produces inert
  content (objects, a change worktree, a squash commit nothing points at). The
  commit point is a single ref move.
- **A2 — CAS against what you read.** Every ref move names its expected old
  value, and that value is the one the work was **derived from**, not a fresh
  read taken at flip time. A window between the read and the flip is a lost
  update, whatever guards sit inside it.
- **A3 — Failure is a non-event.** A rejected commit point leaves observable
  state exactly as the op found it. Anything the abort path or a retry reads must
  survive the failure unchanged.
- **A4 — Identity is carried, not re-derived.** Any fact the abort path needs —
  above all *which ball this op is about* — is an op input, never re-read from
  mutable scratch. (git writes `MERGE_HEAD`; it does not infer the merge from the
  index.)

Together with converge-on-retry these are self-enforcing: A1+A2 make a lost race
a clean rejection, A3 makes the rejection invisible, A4 makes the abort path
work without the scratch state A3 protects.

## 3. Audit — every op, every anvil

An "anvil" is one repository whose ref an op moves. Verdicts are against §2.

| op | anvil | commit point | CAS | verdict |
| --- | --- | --- | --- | --- |
| `create` `claim` `unclaim` `update` `import` | store branch | `Git::seal` — commit in the change worktree, then `merge --ff-only` (`src/git.rs`) | yes — ff-only rejects a moved tip | **atomic**, gaps G2 (A3/A4), G3 (voice) |
| `conf` `install` | landing branch | same seal, landing as anvil | yes | same; contention is rare (single-owner, §4) |
| `conf set task-remote` / `clock-provider` | none — `binding.toml` | write-temp + `rename` (`src/conf_write.rs`) | **no** | G4 CLOSED (bl-ffbf) — atomic replace now; the lost update is named and accepted (§1) |
| `close` | ① project repo | `commit_swap` — `update-ref <int> <new> <old>` (`src/delivery_repo_acts.rs`) | old value read AFTER the gate | **G1 — violates A2** |
| `close` | ② store branch | `Git::seal` | yes | atomic; ordered after ① so a retry converges (`Standing::Settled` skips the squash) |
| `close.post` push | remote | tracker push | yes — non-ff reject | atomic; converges on `sync` + retry |
| `sync` | store branch | `merge --ff-only FETCH_HEAD` / ff refspec | yes | atomic; "a partial sync leaves the branch at the old or the new tip, never wedged"; the merge's refusal speaks balls' voice (bl-3129) |
| `prime` founding | landing/store | the landing seal — the predicate is a COMMIT, not a directory | the commit is the only observable | G5 CLOSED (bl-ffbf) — a crashed founding is re-runnable debris, not a brick |
| `bl-chore` `claim.post` | store branch | a NESTED `bl create` — its own commit point | yes (its own) | G6 CLOSED (bl-ffbf) — still outside the parent atom, but the rollback now deletes the orphan (§14 appendix) |
| op log | `clones/<enc>/log` | `O_APPEND` under `PIPE_BUF` | n/a | atomic by construction (§1) |
| `seen` tokens | XDG state | `fs::write` (`src/seen.rs:214`) | n/a | a cache; a torn write costs one spurious refusal |

Everything else in the codebase reaches its anvil through `Git::seal`, so the
audit is complete at the seam, not per verb.

## 4. G1 — the delivery CAS validates the wrong old value (A2)

`Project::deliver` (`src/delivery_repo_acts.rs`) runs, in order:

1. `reintegrate(path, integration)` — fold `integration` into `work/<id>`. **Reads
   the integration tip at T0.**
2. `gate(path)` — the project's own `pre-commit` hook. 10-13 minutes here.
3. `ensure_no_resurrection(root, branch, integration)` — **re-reads the tip, T1.**
4. `commit-tree` with `tree = work/<id>^{tree}`, `parent = rev-parse integration`
   — **re-reads the tip, T2.**
5. `commit_swap(..., commit, parent)` — CAS with `parent` (T2) as the old value.

The gated tree is a function of the tip at **T0**. The CAS validates the tip at
**T2**. bl-a3bb correctly closed the T2→flip window; the T0→T2 window is still
open and it is the one the gate sits in. Two consequences:

- **False abort (reported as MODE 1).** A sibling close lands mid-gate. Its paths
  are not in this branch's authored set, so step 3 sees them as excess and aborts
  naming them a *resurrection*. Observed 3x by one agent. The abort is right; the
  reason given is wrong, and a whole gate run is spent to reach it.
- **Silent revert (not reported; worse).** If the mover's changed paths are a
  **subset** of the paths this branch authored — two agents editing one shared
  file — step 3 finds no excess, the CAS at step 5 passes against the post-move
  tip, and the squash tree (computed from the pre-move fold) **reverts the
  sibling's landed change**. No error. The only trace is the squash itself.

The no-resurrection invariant is currently the *only* thing standing between a
mid-gate advance and a lost update, and it holds only for the path sets that
happen not to overlap.

**Fix (bl-8b89): pin the base.** Read `base = rev-parse(integration)` **before**
the fold; pass it as `commit-tree`'s `-p`, as `commit_swap`'s old value, and as
`ensure_no_resurrection`'s comparison point. Then the CAS validates exactly what
the gate was run against, a mid-gate move is one clean rejection in bl-a3bb's
existing voice, and the resurrection invariant goes back to detecting only
resurrections. One variable, read earlier; no new state, no new flag.

**Closed out (bl-9522): the fold consumes the pin too.** bl-8b89 left the
`merge` reading the REF NAME, so the pin and the fold were two reads of one ref
with a sub-second window between them. Forward movement in that window was
already safe (the CAS on the pinned base rejects cleanly); the unsafe shape was
contrived — `integration` RESET back to the pin after the fold read a newer tip,
so the CAS passes and delivers content integration no longer wants. It is closed
structurally rather than argued away: `reintegrate` takes the SHA, so ONE read is
the whole delivery's notion of where integration was, and there is no second read
left to disagree. The ref NAME survives in the delivery-conflict voice beside the
pin (*delivery conflict merging `main` (pinned at `<sha>`) into the work branch*)
— the operator thinks in branches, the delivery acts on a commit. **No residual
window remains here.** The window itself was never testable without a seam
between the `rev-parse` and the `merge`, and adding one to prove a race that no
longer exists is the wrong trade; the deterministic receipt is asserted instead —
git records `Merge commit '<pinned sha>'`, which `git merge main` could not
produce.

## 5. G2 — a failed seal destroys the state its own abort path reads (A3/A4)

MODE 2, root-caused. `Git::seal` (`src/git.rs`) is two acts: `add -A` + `commit`
in the change worktree, then `merge --ff-only` on the checkout. When the ff loses
to a sibling seal, `seal()` resets the **checkout** (bl-07d6's fix) and returns
`Err` — but nothing resets the **change worktree**, which is now committed and
clean, and `trace.seal` stays `None` because `seal()` never returned `Ok`
(`src/lifecycle.rs`). So:

- the engine unwinds as a **pre**-abort, whose rollback wire carries no
  `metadata` (there is no seal record);
- `bl-delivery`'s edge calls `delivery::resolve_id`, whose fallback scans the
  change worktree for the single changed `tasks/<id>.md` — and finds **zero**;
- `expected exactly one changed task file, found 0` → the rollback exits non-zero
  → `plugin bl-delivery rollback failed … its close.pre side effects may not be
  unwound`.

The state is in fact fine: the squash stands on `main` (BINDING, correctly), the
ball is still open, and the retried close converges (`Standing::Settled` skips
the squash). The operator is told to distrust it. `src/git_tests.rs`
(`a_lost_seal_resets_the_checkout_so_later_ops_succeed`) asserts the checkout is
restored and says nothing about the change worktree — the hole is visible in the
test's own scope.

§7 already named this class for the post-abort case (bl-430e: "the post-abort
change worktree is clean, so a pre-phase rollback starved of the trailer has no
staged task file left to re-derive its id from") and fixed it by putting
`metadata` on the rollback wire. The failed seal is the **third** state that fix
did not anticipate: *committed, not integrated, no seal record*. The bl-cf93
narration abort is a fourth (clean worktree, no seal record).

**Fix (bl-a5f3) — SHIPPED.** Carry the id (A4): the ball rides `command.id` on
the pre wire — it is op-constant and core always knows it (the verb names it;
`create` mints it) — and `resolve_id`'s changed-file fallback plus
`delivery_repo::changed_task_paths` are DELETED. No plugin derives *which ball*
from mutable staging state any more, and pre / failed-seal / post all carry one
id. This subtracted a code path rather than adding one. `tests/lost_seal.rs`
constructs the losing state end to end (a conformant `close.pre` plugin wired
after `bl-delivery` commits to the store checkout, so core's ff cannot win) and
asserts the unwind is clean, the squash stands, the ball stays claimed, and the
retry converges to exactly one delivery.

*Rejected:* special-casing `rolling_back` with an empty changed set into a silent
no-op. That branches on a symptom; the missing reframe is that identity was never
scratch state. *Also rejected as unnecessary:* making `seal()` restore the change
worktree on ff failure (A3 literally). Once nothing reads that worktree, its
state is unobservable — the rollback discards it either way.

## 6. OPEN — starvation is not solved by G1

Tracked as **bl-9042** now that bl-cdec (the umbrella this document was filed
from) is closed with every other mode fixed; the dialogue converges there, and
the evidence to gather first is the observed loss rate under traffic *now that
aborts are clean* — the 3x-per-agent count predates bl-8b89's pin.

G1 converts a wasted gate into a *clean, immediate* rejection at the flip, but the
gate is still re-run on every retry, and under sustained traffic an unlucky close
can lose repeatedly. The report asks for a bounded re-fold loop or a delivery
lease. Three options, stated maximal-subtraction first:

1. **Nothing beyond G1.** The abort is honest and immediate; the operator
   retries. Cost: O(n²) wasted gate-minutes at n concurrent closes, and no
   fairness guarantee at all.
2. **Bounded internal re-fold + re-gate.** Mechanism inside `deliver`, unbounded
   wall-clock, and it does not fix fairness — a loser can lose every round.
3. **Serialize the gate.** Take a lease before the fold, release after the flip.
   The lease need not be new state: `update-ref refs/balls/delivery/<integration>
   <work-commit> ''` is an atomic create-if-absent (empty old value = must not
   exist), release is a delete, a stale lease is a ref `prime` can report as
   debris exactly like an orphan worktree. This is git's index.lock discipline one
   rung up, and it costs **no throughput** when the box is CPU-saturated — which
   is precisely why those gates took 10-13 minutes.

Leaning (3), but it is the one place here that adds mechanism, and a lease wants a
staleness story (a holder that dies mid-gate). Worth attacking before building:
option (1) plus G1 may be enough, and is free.

Separately: whether the store seal's contention deserves an in-core bounded
retry. RESOLVED **no** (bl-fa89) — converge-on-retry is the rule and the retry is
one command; an in-core loop hides contention and doubles wall-clock on a real
conflict. The work was the voice, and it SHIPPED: `Git::seal`'s ff-only rejection
now returns one balls sentence — *the store moved under this op — a concurrent
`bl` won the seal; nothing was written. Re-run the command…* — for EVERY spelling
of the loss (`Not possible to fast-forward`, `cannot lock ref HEAD`, `Your local
changes would be overwritten`), because the fact and the remedy are identical in
each and only the raw text differed. Detection was already the existing `Err`
path; no mechanism was added.

The same gap one layer out — `sync`'s ff-only import — is FIXED too (bl-3129):
*`<remote>`'s `<branch>` moved and this store could not take the fast-forward —
nothing was imported and nothing local was changed. Re-run `bl sync`…* One
DIFFERENCE, and the sentence carries it: a refused import is not always
transient. The optimistic cycle un-seals a rejected push (tests/claim_race.rs),
so the ordinary cause is a concurrent `bl` whose seal was in flight across the
fetch and a re-run converges — but a store that really holds an unpublished
commit keeps refusing, and saying only "re-run" would send the operator into a
loop. Naming both readings is what makes it an instruction. Again no mechanism:
no probe of WHICH case it is, no retry.

## 7. Gaps, filed

| gap | ball | severity |
| --- | --- | --- |
| G1 delivery CAS validates the wrong old value (A2) | bl-8b89 | **high** — silent lost update |
| G2 failed seal + re-derived identity (A3/A4) | bl-a5f3 | **high** — false alarm, unwind reports failure — FIXED |
| G3 store-seal contention speaks git's voice | bl-fa89 | medium — legibility — FIXED |
| G4 `binding.toml` read-modify-write | bl-ffbf | low — **FIXED**: temp + `rename`; the lost update stays, accepted in §1 |
| G5 founding crash window (`is_landing` = a directory, not a commit) | bl-ffbf | low — **FIXED**: the predicate is a commit on the landing branch, and founding re-runs over the debris |
| G6 `bl-chore`'s nested `create` is outside the parent atom | bl-ffbf | low — **FIXED**: `rollback claim.post` closes what that claim minted (§14 appendix); the nested op still seals outside the atom, which is the appendix's premise, not a defect to remove |

## 8. Test obligations

Each obligation is checkable, and none of the checks needs a real race — the
losing state can be constructed:

- **A2:** advance `integration` between the fold and the flip (a fixture commit
  after `reintegrate`, before `deliver` returns) with the mover touching a path
  the branch also authored; assert the close ABORTS and the mover's content
  survives on `integration`. This is the silent-revert case and has no test today.
- **A3/A4:** DONE — `tests/lost_seal.rs` makes the ff fail with the real close
  chain wired and asserts the unwind names no `changed task file` and no
  `rollback failed`. (A3 literally — restoring the change worktree — stayed
  unbuilt: once nothing reads that worktree its state is unobservable.)
- **A1:** already covered — a rejected CAS leaves `integration` unmoved
  (`commit_swap`'s tests).

`tests/` is coverage-neutral (only `src/` counts), so the multi-process race
cases belong there without fighting the 100% gate.
