# bl-1266 — a nested op must not publish

**OPEN (2026-08-06, Jawbreaker).** Filed from bl-1266, found while landing
bl-ffbf. §14 says *"core never pushes, so there is nothing remote to chase."*
That stopped being true the moment a plugin shelled `bl`. This document traces
the failure exactly, names the invariant that actually broke, and argues that
the fix is a **subtraction** — one condition that stops a nested op from
publishing — rather than the compensating-rollback reframe the ball leans
toward. §7 is the part that is NOT settled and wants the maintainer's attack.

**Status: the RULE (§3) is IMPLEMENTED and closed bl-1266 (2026-08-12, main
`9f3d1bf4`); the FOLD (§4) is IMPLEMENTED and closed bl-1da3 (2026-08-12, main
`b45c37ba`).** The record stays OPEN for exactly one thing, and it is the one
nothing can currently reach: H1's cross-repo fill (§3.1). The fold's own
by-product is that no shipped plugin shells `bl` at all any more, so the hole is
now unreachable by construction rather than merely unexercised.

**Second pass (2026-08-11, Enthused).** Every premise above re-verified against
`main`: the wiring still reads `claim.post = bl-chore, bl-delivery, bl-tracker`
and `create.post = bl-tracker` (`bl conf`); `Subprocess::spawn` still exports
`BALLS_PLUGIN_DEPTH = depth + 1` and exports *only* protocol/name/depth/clock,
no store path (`src/plugin.rs`); `change.rs`'s `Occupancy::stage` still refuses
a claim by name with no self-exemption; `src/chore_scratch.rs` still exists and
bl-chore is still the only `bl`-shelling site in the tree. **No sibling plugin
shells `bl` at all** — not balls-github-plugin, not balls-adversary, not
bl-workhours — so H1 is hypothetical among everything shipped. §7 is rewritten
from bare questions into positions; §3.2 is new.

## 1. The failure, exactly

Against this repo's real wiring — `claim.post = bl-chore, bl-delivery,
bl-tracker` (`bl conf`), with the tracker last per §14's *"un-undoable
side-effects sort LAST"*:

1. `bl claim X` seals the claim to `balls/tasks`. Store tip moves `C0 → C1`.
2. `claim.post` → **bl-chore** shells `bl create` (depth `1 → 2`). That nested
   op seals **C2** on the same branch, then runs its OWN `create.post` →
   **bl-tracker** → `git push origin balls/tasks`. **The remote now holds C1
   and C2.**
3. `claim.post` → **bl-delivery** fails. Any reason will do: the worktree path
   is occupied, the project repo moved, the disk is full.
4. §14 unwinds. bl-chore's rollback closes the minted children (bl-ffbf — yet
   another nested op, yet another push, **C3**), then core un-seals:
   `git reset --hard C0` on the **local** store (`src/git.rs`, `unseal`).
5. Local store: `C0`. Remote: `C0←C1←C2←C3`. The next `bl sync` fast-forwards
   cleanly, because it is a legitimate fast-forward. **The claim is back** —
   the ball reads claimed, by an agent that holds no worktree.

The retry dead-ends in the ordinary way. The operator re-runs `bl claim`, which
seals a second claim commit locally; its push is non-ff, so E5 says *"run `bl
sync`, then re-run the command"*; `bl sync` imports the aborted claim; the
re-run now hits `change.rs`'s guard — *"claim: X is already claimed by
Jawbreaker"* — which refuses even the original claimant. The real remedy is
`bl unclaim` then `bl claim`, and no message on that path says so.

## 2. What actually broke

§14's sentence is about core, but the invariant it stands for is not:

> **The remote never carries a commit this store does not want.**

That held for one *structural* reason, not by policy: the tracker's push is the
LAST hook in every post list, so nothing could run after it and fail. Nesting
inserts a whole op — seal AND push — into the middle of the parent's post
phase. Two faults, stacked:

- **Local.** A nested op makes a SECOND commit point on the parent's OWN anvil,
  inside the parent's window. bl-cdec §2's guarantee is *"exactly ONE commit
  point per repository"*; nesting breaks it on the nose. Locally it is benign
  only by accident — `unseal` is `reset --hard C0`, which happens to discard C2
  and C3 along the way.
- **Remote.** The nested push takes that state beyond the reach of a local
  reset. This is the one that bites, and it is the ball.

A third, unstated: nesting is already an exception to **bl-057a** (one HEAD per
op — *"nothing advances the store branch by plumbing behind it"*). The nested
seal advances the store branch behind an open parent op. It survives today only
because bl-chore is wired to `post`, after the parent's own seal. Wire the same
plugin to `claim.pre` — wiring is config, and nothing forbids it — and the
parent's `merge --ff-only` loses to its own child. That is a clean abort, not
corruption, but nothing anywhere says so.

## 3. Maximal subtraction — a nested op does not publish

**The rule: an op publishes only if it is the outermost `bl` in its invocation
tree.** One condition, in the tracker's `push` handler.

The signal already exists and already travels. `BALLS_PLUGIN_DEPTH` is the §6
recursion guard: core's edge reads it (`Edge::depth`, `0` at the top level) and
`Subprocess::run` spawns each plugin with `depth + 1` (`src/plugin.rs`). So a
tracker invoked by a top-level `bl` sees `1`; a tracker invoked by a `bl` that a
plugin shelled sees `2` or more. `remote_ops::push` no-ops above `1`, read at
`src/bin/bl-tracker.rs`'s edge, which already reads env there.

What that buys, all of it structural:

- The nested create's C2 seals locally and rides the **parent's own** trailing
  push. One push per op *tree*, and it is still last — §14's "sorts last" now
  holds across nesting instead of only within one op.
- Abort before that push ⇒ nothing was published ⇒ core's local reset is again
  a complete undo. **§14's sentence becomes a theorem rather than an accident.**
- bl-chore's rollback stops pushing too, which is correct: there is now nothing
  published to compensate for.
- The E5 path stops leaving an orphan. Today the nested push has already
  published the claim by the time the parent's own push is rejected.

Cost: no new env, no wire field, no core change, no config. One condition.

### H1 — cross-repo nesting is over-suppressed

A plugin that shells `bl -C otherrepo create` publishes to a DIFFERENT store
with a different remote. Depth suppresses that too, leaving the far store
sealed-but-unpublished — bl-4945's shape. The parent's push cannot cover it:
git offers no cross-repo atomicity, and bl-cdec §2 already concedes the ceiling
(*"atomic in each and convergent across them"*). So depth is a coarser
predicate than the true rule, which is:

> **An op does not publish an anvil an enclosing op holds open.**

The fill, if H1 becomes real: core exports the store path it holds into every
plugin spawn (inherited through the shelling plugin, exactly as depth is), and
the tracker suppresses iff `binding.store` matches it — failing OPEN when
unset. That is a new env, which §6 explicitly calls a smell (*"no
`BALLS_LOG_DIR` — a new env is a §0 smell"*). It is also the only channel that
reaches a nested core: bl-chore spawns the nested `bl`, so core cannot put
anything on that command line.

**Sharpened (second pass).** "Coarser predicate" undersells it. Depth-suppression
is safe exactly when the nested op writes the SAME anvil its parent will push,
because then the parent's push carries the child's commit for free — a push
publishes a branch TIP, so C2 rides C1's push with no bookkeeping and the debt
is discharged by construction. Cross-store nesting is not a coarse call on the
same rule; it is **a debt nobody pays**: the parent pushes its own anvil, the
far store stays sealed-and-unpublished, and no later op in *this* repo will ever
cover it. So H1 is a real hole, not a rounding error — it is just an
unreachable one today.

**Still not proposed now.** Nothing shipped shells `bl` except bl-chore, which
shells it in-store. The fill is written down (§3, above) so that whoever first
writes a `bl -C` plugin finds the answer rather than the bug; the cost of
carrying it early is a new env §6 calls a smell.

### H2 — a leaked `BALLS_PLUGIN_DEPTH` silently stops publishing

Already a live hazard in this repo (integration tests must scrub inherited
`BALLS_PLUGIN_DEPTH`/`BALLS_PLUGIN_NAME` before shelling `bl`). Today a leaked
depth only eats into the recursion cap, which is benign. Under this rule it
makes a top-level `bl` stop pushing, silently.

**Position (second pass): accept — because "silently" is the only real
complaint, and balls already owns the surface that un-silences it.** Do not pay
for the held-store env; it fails open on a stale value far more often than a
stale depth does, and it buys nothing H2 actually asks for.

The reframe that makes acceptance principled rather than a shrug: **a nested op
is not a new state, it is a rung on the publish ladder balls already has.** §12
is one ladder answering one question — *does this op publish, and on whose
authority* — and `bl conf` already renders it as value + provenance, with
`(none)` from `landing` (declared stealth) distinct from `(none)` from `stealth`
(circumstantial — nothing set, no origin; `src/conf_resolve.rs`). Nesting is
simply a third way to read `(none)`:

```
task-remote     (none)     nested
```

*This op does not publish because an enclosing `bl` holds the store open.* No new
concept, no new report, no new verb — one more provenance value on a line that
already exists to answer exactly this question. A leaked depth then presents as
a repo that reads stealth when the operator expects it to publish, which is a
diagnosis one `bl conf` away, and the store publishes anyway on the next clean
op (local-ahead is forward-only, §6).

The one place the analogy is NOT exact: declared stealth owes no push ever,
whereas a nested op owes one that someone else pays. Same-store nesting always
collects (above); cross-store never does (H1). The rung is honest only while H1
stays unreachable.

**RATIFIED 2026-08-12 (maintainer): accept the overload, on condition that the
inexactness is written down** — *"I think that's okay for now. Just make sure
it's documented."* So the caveat is not a note in this record; it is a
DELIVERABLE of the implementing ball, and this section names the four places it
must land and the sentence it must say. Without it the rung is a trap: a reader
who has internalised `(none)` ⇒ *this checkout never publishes* will read a
deferred push as a disabled one.

The sentence to carry, in balls' own voice:

> **`(none)` from `nested` is NOT stealth.** It means an enclosing `bl` holds
> this store open and will publish for this op. Every other `(none)` says
> nothing will be published; this one says the push is OWED, and the outermost
> op in the invocation tree pays it. Seeing `nested` at top level means a
> `BALLS_PLUGIN_DEPTH` leaked into this shell — nothing is lost, the store
> publishes on the next clean op.

The four sites, all verified present on `main` (2026-08-12):

1. **`skill/conf.md:19`** — the one that becomes FALSE, not merely incomplete:
   *"a checkout with no durable remote shows `task-remote (none)` — that
   checkout is stealth."* Under the rule a nested op reads `(none)` and its
   checkout is **not** stealth. This line must be qualified, or it teaches the
   exact misreading above.
2. **`docs/architecture.md` §4's `bl conf` provenance paragraph** — enumerates
   the readouts as a closed set: *"the two are DISTINCT readouts (bl-9df0):
   declared stealth reads `(none)` from `landing` … circumstantial stealth reads
   `(none)` from `stealth`."* "The two" becomes three, and the third is not a
   kind of stealth — the sentence needs restructuring, not an appended clause.
3. **`src/conf_resolve.rs::task_remote`'s doc comment** — same closed-set claim
   at the code: *"Three distinct no-remote readouts (bl-d234): declared,
   unset-with-origin, unset-without."* Becomes four, and the new one is the odd
   member.
4. **§12's durable ladder** — where the rung is defined at all. This is the one
   place the *why* belongs (an op does not publish an anvil an enclosing op
   holds open, §3.1's H1), rather than only the readout.

Sites 2 and 3 both state a CLOSED SET and both cite the ball that closed it, so
neither can be extended by a footnote; that is the tell that this rung is a real
addition to the ladder rather than a rendering detail.

### §3.2 — the rule makes "the publisher sorts last" load-bearing

§14's *"un-undoable side-effects sort LAST"* is today a hygiene convention:
core is semantics-blind (§0) and cannot know which plugin publishes, so hook
order is the operator's config. Under §3 that convention becomes a
**precondition for completeness**. Wire `claim.post = bl-tracker, bl-chore` and
the parent publishes C1, then the nested mint seals C2 which no push in this op
covers — the store sits local-ahead until the next op publishes it.

That is a deferred push, not a lost one, and local-ahead is precisely the
forward-only residual §6 says is the only surviving divergence. So: **accept and
document, do not enforce.** Enforcement would require core to know which plugins
publish, which is the §0 line and the same reason §7.5 rejects a core-side rule.
Worth saying out loud in §14 beside the sort rule, because the cost of getting
the order wrong changes from "an un-undoable effect ran before an abort" to
"…and a commit waits for the next op."

## 4. The deeper subtraction — bl-chore should not nest at all

§3 fixes the class. For the one *shipped* instance the whole mechanism is
removable, and it is removable because §14 filed it under the wrong heading.

§14's appendix is for effects whose binding artifact lives in an EXTERNAL
system — the jira ticket. bl-ffbf extended it: *"A NESTED `bl` OP IS THE SAME
CASE … balls itself the 'external tracker that assigns its own id'."*

**It is not the same case.** balls is not external to itself. The appendix
exists precisely because core cannot reach into jira to make the ticket part of
the atom. Core *can* reach the store — that is the change worktree, and `pre`
is the sanctioned door. §8 step 2: *"pre modifiers … edit the shared worktree
(rename the ball file to reassign an id, edit frontmatter)."* §8.3's
seal-validation already contemplates exactly this: *"a `pre` plugin edits the
SHARED change worktree, so it can also touch a SIBLING `tasks/*.md`."*

So bl-chore's mint belongs in `claim.pre`, writing files, not in `claim.post`,
shelling ops:

- write `tasks/<child>.md` for each chore into the change worktree;
- add the `{id, on: close}` blocker to the parent's file — already in that
  worktree, since `claim` is staging its `claimant` there.

One seal, one commit, one push. Claim's `finalize` checks only its own shape
(`src/lifecycle_validate.rs`: *"Each verb's finalize checks only the op's OWN
shape"*), so the extra file passes; only `create` carries an
exactly-one-new-file check.

**What it deletes:** the nested `bl create` and the mint's `Bl` shell seam;
`src/chore_scratch.rs` in full (the ids never cross a process boundary, so
there is nothing to carry); bl-chore's rollback and its mid-list inline unwind;
§14's nested-op paragraph; and this ball's failure mode for the shipped case.

**What it costs, and must be decided:**

- **A plugin minting an id.** The scheme is fixed and public (§ id generation:
  `bl-` + 4 lower hex, collision re-rolls off the LIVE set), and the plugin can
  read the live set from the worktree it is standing in. But the current split
  is *core mints, plugins reassign*; a `pre` plugin minting a fresh id for a NEW
  ball is a seam that does not exist yet.
- **The journal.** Today the child's birth is its own `create` commit and the
  journal renders from history. Folded, the child is born in a commit whose §5
  subject is `claim <parent>`, so the child's journal would read "claim" as its
  creation event. Either §5 grows a way to name more than one act per commit, or
  the wart is accepted. A render wart, not a correctness break.
- **Generality.** This fixes bl-chore, not third-party nesting. §3 is still the
  backstop. They are severable and should ship separately: §3 closes this ball,
  the fold is its own.

## 5. Rejected

**(b) sync becomes convergence-aware of aborted ops.** Dead on arrival. `sync`
cannot tell an aborted claim commit from a real one — it is well-formed and
byte-identical to a claim that stuck. Distinguishing them means marking aborts
ON the remote, which means pushing a compensation, which is (c). A branch on a
symptom, and the symptom is not even detectable.

**(c) rollback SEALS a compensating unclaim instead of resetting.** The ball
leans this way. It should not survive contact:

1. **It costs §14 its one infallible act.** *"Core's tier-1 un-seal always
   succeeds (local), so the op's core invariant holds even if every plugin
   rollback fails."* A compensation must be PUBLISHED to compensate anything,
   and a push can fail. Tier 1 stops being the floor everything else stands on.
2. **It does not generalize past `claim`.** A published-then-aborted `close`
   compensates by re-creating a ball balls deliberately has no verb to re-create
   (no `reopen`; the round trip is `show --json | import`). A
   published-then-aborted `update` compensates by restoring a prior value
   nothing carries. "Rollback compensates" needs a per-verb inverse — precisely
   the per-op special-casing §14 spent itself dissolving.
3. **It re-admits half-states as a class.** §14's claim is that they are
   *"immaterial BY CONSTRUCTION"*. A compensation window is a state a reader
   can observe and must interpret.

Its one true insight survives, and it is the whole argument for §3: **once an
effect is published, undo is off the table and only compensation exists.** The
correct response is not to get good at compensating — it is to not publish
inside an atom that may abort. §3 dissolves (c) rather than answering it.

## 6. What this settles for bl-4945

bl-4945 asks which side owns reconciliation. Under §3 the answer is structural:
**the local store does, and there is only ever one direction to reconcile.**

The remote can only ever be behind-or-equal — never ahead with an op the local
store repudiated:

- push succeeds ⇒ remote == local;
- push fails ⇒ the op aborts, local resets to the pre-op tip, and the push that
  failed published nothing;
- crash between seal and push ⇒ local ahead, remote behind — bl-4945's state,
  and the only residual.

The bl-1266 direction becomes **unconstructible**, not merely unlikely. So
bl-4945's dead end is forward-only: the remedy is a push, never a revert. Its
E5 sentence can then honestly name the two states apart — contention ⇒ `bl
sync` + retry; unpublished-local ⇒ publish — instead of advertising `bl sync`
for a state sync cannot fix.

## 7. Open — what wants attack

Second pass: each carries a position now. Attack the positions, not the
questions.

1. **H2 — a leaked depth. RESOLVED 2026-08-12 (maintainer): accept, as a rung
   on the §12 publish ladder** (`task-remote (none) nested` in `bl conf`,
   alongside declared and circumstantial stealth) — *"I think that's okay for
   now. Just make sure it's documented."* The held-store env stays refused: it
   fails open on a stale value more often than depth fails closed, and the
   grievance is silence, not derivability. **The condition is binding**: the
   overload — every other `(none)` means *never publishes*, this one means
   *someone else publishes for me* — ships documented or not at all. §3.1 names
   the four sites and the sentence; two of them state a closed set that a
   footnote cannot extend, and one (`skill/conf.md:19`) goes from incomplete to
   FALSE. *"For now"* is load-bearing too: the rung is honest only while H1
   stays unreachable, so whoever makes `bl -C` nesting real reopens this.
2. **H1 — cross-repo nesting. POSITION: defer, with the fill written down.**
   Verified 2026-08-11: no shipped plugin shells `bl` at all. It is a debt
   nobody pays rather than a coarse predicate (§3.1), so it is a real hole —
   but an unconstructible one, and closing it costs a new env §6 calls a smell.
   **The attack: is "write the fix down and wait for a caller" acceptable for a
   hole we know is real, or does an unreachable-but-known hole get closed?**
3. **The fold's id seam. POSITION: the question is smaller than it looks — the
   power already exists.** A `pre` plugin can already write arbitrary files into
   the shared change worktree (§8 step 2 blesses editing it; §8.3 concedes it
   reaches SIBLING `tasks/*.md`), the id scheme is public and fixed, and the
   live set to re-roll against is in the worktree the plugin stands in. So
   nothing gates minting today; there is no seam to open, only doctrine to
   write — one sentence in §8, not a mechanism. Ordered plugins even compose:
   a later `pre` sees an earlier one's file when it re-rolls. **The attack: is
   "core mints, plugins reassign" a boundary worth defending on purpose, given
   that nothing enforces it?**
4. **§5 and multi-act commits. POSITION: dissolve — there is no wart.** The
   objection is that a folded child would be born in a commit whose subject is
   `claim <parent>`, so its journal reads its creation as "claim". That is
   accurate: the child exists *because* the parent was claimed, and the folded
   commit is one act. A synthetic `create` line would be the less truthful
   render. §5 needs no change and the fold does not wait on one.
5. **Where the rule lives. POSITION: confirmed, tracker-side** — and the §3.1
   reframe strengthens it: *when to publish* is already a ladder the tracker
   owns end to end (§12), so nesting is a rung on an existing decision rather
   than a new one placed anywhere. Core-side remains rejected: it requires core
   to know which plugins publish, which is the §0 line.
6. **The retry message. POSITION: no message owed if §3 lands.** §3 makes the
   remote-holds-a-repudiated-claim state unconstructible, and that state is the
   only way to reach the dead end. `bl claim` refusing its own claimant remains
   correct for the *other* path to a stale self-claim (sealed, then the session
   died), where `unclaim` + `claim` is the intended takeover and SKILL.md's
   abandonment section already says so.

## 8. Sequencing

Three severable pieces, in dependency order — only the first closes this ball.

**Piece 1 is BUILT (2026-08-12), and it is what closes bl-1266.** As shipped:
`tracker::Env` carries the §6 depth (parsed in the lib, `Env::resolve`, failing
OPEN on absent/garbage so a hand-run tracker still publishes) and answers
`Env::nested()` — `depth >= 2`, the arithmetic stated once, since core spawns
plugins at its own depth `+ 1`. `remote_ops::push` returns early when nested;
`prime_post`'s ESTABLISHED-branch push inherits it, while its FOUNDING push
deliberately does not (founding is what makes a store publishable at all, and no
enclosing op's established push can stand in for it). `conf_resolve::task_remote`
preempts every durable tier with `(none)`/`nested`. Four documentation sites
carry the caveat as §3.1 requires. Tests: a nested push leaves the remote tip
untouched where the identical top-level push moves it; the depth parse's
fail-open; the rung's four depths; and the `conf` readout preempting a configured
binding remote. `remote_ops_tests.rs` split at the 300-line cap — `push` and its
rules now live in `remote_ops_push_tests.rs`.

1. **The rule** (§3): one condition in the tracker's push, plus the `nested`
   provenance rung in `bl conf` (§3.1) and the sort-order note in §14 (§3.2).
   No core change, no wire change, no new env. **The four documentation sites in
   §3.1 are part of this piece, not follow-up** — the maintainer's ratification
   of the rung is conditional on them, and `skill/conf.md:19` becomes false the
   moment the code lands, so a delivery that ships the condition without the
   prose ships a documented lie. Two of the sites (§4's provenance paragraph,
   `conf_resolve.rs`'s doc comment) assert a CLOSED set of no-remote readouts
   and must be restructured rather than appended to.
2. **The fold** (§4): **BUILT (2026-08-12, bl-1da3, main `b45c37ba`).** bl-chore
   mints in `claim.pre` by writing `tasks/<child>.md` plus the parent's blocker,
   deleting the nested `create`, the `Bl` seam and `src/chore_cli.rs`,
   `src/chore_scratch.rs`, the rollback and its inline unwind, and the
   `close.post` record sweep — net −402 lines. epic-skip's `bl list --json` went
   with it (the children are in the worktree). §7.3 was the only prerequisite and
   settled as predicted: nothing gated a `pre` plugin authoring a ball, so the
   answer was a doctrine sentence, now in architecture §8. The child's clock is
   the parent's freshly-staged `updated` and its `root_commit` is the parent's —
   neither fact is derived twice. Verified live: one commit carries both the
   claim's edit and the child's birth.
3. **H1's fill** (§3.1): only if a `bl -C` plugin ever appears.
