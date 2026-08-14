# bl-24e7 — Speculative merge queue: pre-gated prefix landing

Status: **CONVERGED** (dialogue 2026-08-11, landed 2026-08-12). This file is
the artifact; the ball records the work. Edit this like code.

Implementation: **bl-1263** (verdict cache — the foundation), **bl-5c5f**
(merging-queue semantics), **bl-d0c2** (speculator + scheduling),
**bl-6312** (GH Actions remote builder). Open questions 2–4 are settled
inside those balls; question 1 dissolved (below).

## Problem

A dozen agents, a ~15-minute close gate (clippy → 300-line → tarpaulin), one
`main`. Closes serialize on the gate, so N ready balls take N × 15 minutes to
land while N−1 agents sit idle. The contention is not the merge (cheap) but the
gate, plus the fold-race: main moves between fold and merge, forcing re-fold and
re-gate.

## Non-goals (attacked and rejected)

- **Opportunistic cross-pulling** (agents merging each other's in-flight
  branches): stale snapshots, gate superposition (your close fails on their
  clippy), provenance break (delivery *is* the `[bl-xxxx]` no-ff merge), and
  N² pairwise coupling — components meet only at the interface, and main is
  the interface.
- **Binary collapse trees** ("exponential" pairwise combining): N−1 internal
  merge nodes that either each pay a gate (more total gates, log-depth serial
  latency) or don't (untested combinations, unattributable failure). Internal
  nodes have no owner — not a ball, no claimant.
- **Hedging the possibility space** (precompiling against predecessor
  failures): 2^(k−1) subsets, no signal about which predecessor fails. Zuul and
  GitHub merge queue do zero hedging. Substitute information for insurance:
  fail-fast gate staging surfaces most evictions in the first minute. (Partial
  exception under free remote capacity — see Speculation policy.)

## The design

**An optimistic prefix queue. No agent ever merges another's code across
ownership lines. Speculation pre-pays gates so closes land in order as cache
hits.** Any topology that adds ungated merge nodes only defers conflict; with
idle agents the objective is latency, not gate-run count.

### Queue = a query over tags

A ball enters the queue by taking a `merging` tag. **Tag-time is queue
position** — order is derived, never stored (no queue file to desync). One
invariant covers seal, eviction, and requeue:

> A branch is sealed while its ball is tagged `merging`; any new commit
> requires dropping the tag and re-tagging.

So a gate-failing culprit cannot fix in place: fixing means committing, which
means retagging, which means bottom of the queue. Eviction has no mechanism of
its own.

**Entering the queue is not an agent's job (bl-b761, 2026-08-13).** A day of
live downstream traffic showed why: 29 of 30 closes paid the full local gate,
because joining was a conscious act — the agent had to know the queue existed,
know its current commit was the last one, and forgo an immediately-available
close to wait on a builder. Every incentive pointed at skipping it, and
fail-open made skipping invisible. The reframe: nobody needs to know which
commit is the last one. The speculator pass **adopts** every `work/<id>` tip
not already sealed at its tip — sealing is the pass's closing act, so a fresh
seal must survive one full inter-pass interval untouched before the next
pass's walk will build it. Quiescence is measured in passes, never against a
clock (an operator's commit-date policy — smearing included — cannot break
it). Sweep-then-adopt across one pass IS requeue-at-bottom for a moved tip,
and an explicit `enqueue` still works and still outranks adoption: its
taggerdate predates the pass that would have adopted it. The invariant above
is untouched — adoption only changes WHO plants the tag.

### Candidates = queue prefixes, computed by merge-tree

Queue [A,B,C,D] yields exactly the candidates A, A+B, A+B+C, A+B+C+D — N
prefixes, never other combinations (kills AB/BA duplicate builds). The agent at
position k builds prefix k: its own branch plus all predecessors — the build it
was going to run anyway, against a richer base.

Candidates are tree OIDs computed by `git merge-tree`: **no branch, no index,
no worktree ever exists**, so there is no ref debris class; unreachable trees
are `git gc` food. The build checks the tree out into a scratch dir, deleted
after.

Parallel prefix gates are **built-in bisection**: if prefix k fails and prefix
k−1 passed, branch k is the culprit, identified inside the same gate window.

### Verdicts = the builder interface

The whole contract between speculators and close is one record:

    { tree_oid, gate_fingerprint, verdict, builder }

- `gate_fingerprint` = hash of toolchain + gate config, so a clippy upgrade or
  rubric change silently invalidates stale verdicts.
- The gate consulted at close hashes the worktree tree and looks it up.
  **Hit → skip the gate. Miss → build locally, exactly stock behavior.**
  "Stated build matches the merge" is inherent in the content-addressed key;
  trust reduces entirely to builder identity. (Corrected 2026-08-12: close
  does NOT fold main — since bl-a1a4 delivery *refuses* a moved main and the
  AGENT folds by merging main into the worktree, then re-closes. The cache is
  indifferent — it keys on the worktree tree, which after the agent's fold IS
  the candidate tree — but the landing choreography is "wake the agent, agent
  merges main (clean by construction: the speculator already proved this
  exact merge), close hits the cache", not an unattended close.)
- The builder is therefore swappable policy: a local speculator loop, a
  sibling box, or GitHub Actions all satisfy the same record. Offline degrades
  to today's behavior, never blocks a close.

### Landing

Balls land **in queue order, each via its own close**. When k's turn comes,
main is exactly the prefix-(k−1) tree, so k's fold reproduces the candidate-k
tree — cache hit, close completes in seconds. Delivery stays the per-ball
no-ff merge: the log is *identical to the fully serialized history*;
provenance, task seal, chore gates, close.post cleanup all run per ball,
untouched. Delete the speculator and you have stock balls, just slower
(severability).

### Scheduling

- Small concurrency cap on speculative builds (memory/IO thrash, not just CPU
  — nice alone doesn't arbitrate `-j nproc` × 12).
- Within the cap, `nice`/`ionice` by queue position; verdicts are consumed in
  queue order, so head first.
- **Depth-reluctance is emergent, not a knob.** A deep position (say 20 of 20)
  should not build yet: the odds its exact prefix survives decay
  geometrically with depth (≈ (1−p)^(k−1) for per-branch failure rate p), and
  its slack before it becomes the blocker grows linearly. But cap + priority
  already produce exactly this: position 20 has no slot until the queue
  drains, a slot frees precisely when a verdict ahead lands, and by then the
  evictions ahead have resolved — **building late is building informed.** The
  just-in-time trigger falls out: a position starts its expensive stage when
  its slack (unresolved positions ahead × observed drain rate) approaches one
  build time. Record the inputs (per-branch failure rate from verdict
  history, drain rate) as metrics, not configuration.
- **Implementation reframe (bl-d0c2, 2026-08-12): strict order zeroes the
  depth-risk entirely.** The shipped speculator builds candidates strictly
  head-first and only ever atop prefixes already holding a PASS verdict —
  evictions are gate failures, gate failures are known, so at build time the
  "will a predecessor evict?" probability the slack formula priced is zero
  (what remains is out-of-order landings and external main movement, both of
  which degrade to an honest cache miss). The slack arithmetic above is kept
  as the reasoning that *led* here, but the implementation needs none of it:
  a conflict or a FAIL verdict ends the buildable chain, and eagerness
  degenerates to **builds-per-pass** — how many gates one speculator pass may
  spend. There is no cross-agent machine cap in v1 (subtracted): passes build
  one candidate at a time under `nice -n19`; the close-time gate on a miss
  runs unniced and so always preempts. Since adoption (bl-b761) the natural
  invoker is an **ambient driver** — a timer or a resident host process
  running passes on a cadence, the cadence being the de-facto debounce — with
  an idle agent's own invocation still lawful and still self-limiting.
- **One declared knob: eagerness.** The metric is computed; *where the
  threshold sits* is a preference the system cannot derive — it encodes the
  owner's watts-vs-wall-time tradeoff. A server with idle cores should burn
  them; a laptop trades wall time for battery. Express it as a single scalar
  S: a position may start its expensive stage when slack ≤ S × build time.
  S = ∞ → build everything immediately (server); S ≈ 1 → just-in-time
  (laptop); S = 0 → speculation off, stock balls. Split cleanly: **capacity
  is measured, preference is declared** — the concurrency cap derives from
  cores/memory, never from preference; S is the only declared value. Default
  S from power state (AC → eager, battery → just-in-time), so the knob is an
  override of a sensible default, not required configuration.
- **A close-time build on cache miss runs unniced and preempts speculators** —
  that is the real merge path.
- Staged fail-fast gate: run clippy + 300-line for *all* candidates up front
  (seconds), pruning the queue before anyone spends the tarpaulin stage.
- One **persistent build dir per agent**, deliberately: successive candidates
  differ by one branch, so warm incremental builds may cut the 15 minutes
  substantially. Swept by prime's debris pass when the agent is gone.

### Speculation policy (severable knob)

What to build, in value order, given capacity:

1. **Prefixes** — mandatory; the scheme is inert without them.
2. **Single-eviction variants near the head** — only when capacity is truly
   free. Eviction of j invalidates every prefix ≥ j, so hedges for small j
   cover the most. This is where idle CPU legitimately goes — but on a laptop,
   "idle" CPU is shared thermal budget: extra builds throttle the head build
   through heat, which no scheduler priority prevents. Locally: prefixes only.
   Remotely (GH minutes, sibling box): hedging becomes defensible, since the
   only cost is money and the local thermal budget is untouched.
3. Nothing else. Unsealed branches are stale snapshots; do not BUILD on
   them. (Adoption — bl-b761 — may SEAL a quiet unsealed tip at pass end;
   the walk still only ever builds what held a seal for a full pass.)

### Cleanup invariants (testable)

After any speculation round:

- zero refs in any speculation namespace (there is none — merge-tree),
- `git worktree list` shows only real claims,
- scratch checkouts deleted; the only persistent artifacts are one build dir
  per agent and the verdict records.

Prime's converge/debris pass is the backstop sweeper, per existing pattern.

## Remote builders (GH Actions)

Fits the verdict interface as-is. Caveats to resolve before wiring:

- Pushing candidate refs re-creates the ref-debris class *on the remote* and
  publishes sealed-but-unlanded code — throwaway namespace with TTL deletion,
  or push the computed tree only.
- **Public repos: standard hosted runners are free with no minutes quota**
  (all plans, per GitHub's published billing docs as of 2026). The caps that
  do exist: 20 concurrent jobs on the Free plan (40 Pro), 6 h per job, and a
  fair-use backstop — GitHub has disabled Actions on accounts for extreme
  CI/CD volume even on public repos. Merge-queue CI for the project itself is
  squarely the intended use (bors ran on free public CI for years); the line
  is non-project compute, not build volume in good faith. Hedge builds
  (speculation-policy tier 2) are where volume grows superlinearly — keep
  them bounded.
- Hosted runners are slow and cold-cached; a 15-local-minute gate may be
  30–40 remote minutes, partially offset by `actions/cache` for cargo. A
  self-hosted runner on owned hardware is the sweet spot: Actions as
  coordinator, warm caches, no laptop thermals, no fair-use exposure.
- Network absence must degrade to local build (it does, by the cache-miss
  path) — a remote builder must never become a close dependency.

## Open questions

1. **Monotone landing rule** — **DISSOLVED (2026-08-12)**: no rule is needed,
   because monotonicity is by construction. Every ball lands only via its own
   close, whose gate consults the tree-keyed cache on the worktree tree its
   OWNER folded (close refuses a moved main — bl-a1a4; the fold is the
   agent's act). If prefix j failed, j's folded worktree is exactly the
   prefix-j tree — it reads the FAIL verdict (or misses), gates locally, and
   fails — while a passing
   prefix k > j cannot land j's code because nothing but j's close lands j.
   If j evicts instead, every deeper candidate's eventual fold produces a
   *different* tree than was speculated — cache miss, honest local build.
   Masking is impossible; queue order itself is advisory (an out-of-order
   close merely misses the cache and pays the stock gate). The special case
   was a missing reframe: per-ball landing + content-addressed verdicts
   already are the invariant.
2. **Conflicted candidates** — **SETTLED (bl-d0c2)** as proposed: the
   candidate is unbuildable and ends the chain; the ball falls back to
   fold-at-close (bl-a1a4: merge main yourself, retry). Resolution is
   judgment and judgment belongs to the branch owner.
3. **Verdict store home** — **SETTLED (bl-1263)**: local XDG state, under the
   `bl-speculate` plugin territory. A verdict is a builder's assertion on
   this trust boundary; publishing it to the center store would widen the
   boundary without widening the trust.
4. **GH wiring details** — **SETTLED (bl-6312)** by a subtraction: the store
   file already IS the wire format (filename = `<tree>-<gate>.toml` key, body
   = verdict), so no remote protocol exists. The runner runs the STOCK gate —
   whose hook records into its own store — and ships the store dir home as an
   artifact; `bl-speculate import` is validate-and-copy, the trust seam.
   `.github/workflows/speculate.yml` triggers on `speculation/**` pushes;
   retrieval (`gh run download` + import) and the branch sweep are manual by
   design — a remote builder must never become a close dependency. Toolchain
   fingerprints do not vouch across versions: a remote verdict hits only when
   `rustc -V` matches, which is the fingerprint working, not failing. Live
   wiring is UNVERIFIED from this box (no network); the workflow is a
   reference implementation.

## Why this shape (philosophy)

Single source of truth: delivery is the merge, order is the tag query,
validity is the content-addressed verdict — nothing stored that can be
computed, nothing duplicated that can drift. Speculation is pure
cache-warming: a capability bolted beside the core, not a change to what
"closed" means. The first design (collapse trees, cross-pulls) added
mechanism; this one adds scrutiny and subtracts it.
