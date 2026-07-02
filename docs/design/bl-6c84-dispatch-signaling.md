# bl-6c84 — dispatch and signaling: how agents learn the store changed

**DRAFT 2026-07-01 (Locations) — awaiting maintainer dialogue.** The maintainer's
framing question (verbatim): *"That's an interesting problem. How do we solve it?
I'm not even really sure what the right mechanism is. A socket that balls can
signal to when relevant state changes occur? What's the right mechanism here?
How does agent signalling happen?"* This doc stakes a position — **zero core
change; signaling is a composition of things that already exist** — and attacks
the alternatives. Open questions for the dialogue are at the end.

## The problem

Nothing tells an agent (or a dispatcher spawning agents) that store state
changed — a ball became ready, a claim released, a gate closed. The only loop
today is poll: `prime`/`sync` + `list` → pick → `claim` → spawn, and every
adopter rebuilds that loop from scratch. Same-box parallel claims additionally
need harness-side serialization (bl-07d6: of 12 physically-simultaneous claim
pairs the loser never once succeeded; it now fails *clean* — `src/git.rs:93-101`
resets the wedge — but still fails and must rerun).

What already works and must not be disturbed: **cross-clone claim contention is
atomic.** The tracker's push is the compare-and-swap — "a non-ff IS the
contention signal" (§13) — the losing op aborts and rolls back, and rerun
converges (§14).

## The reframe that decides everything

**In a git-medium tracker, push can never be the guarantee — only the
optimization.** "Git provides sync; there is no server" (SKILL.md:7): clones go
offline, hooks get skipped, FIFOs have no reader, webhooks get dropped, and no
delivery contract exists anywhere in the medium. So any signaling design that
*needs* the signal to arrive is wrong by construction. The poll loop is not the
embarrassing fallback — it is the correctness floor, and every push mechanism is
a latency optimization that shortens the sleep between polls.

Once stated, the design collapses: build the poll floor **once** (one blessed
reference loop instead of N adopter rewrites), and let any number of optional
poke sources wake it early. Level-triggered, not edge-triggered.

## The invariant: a poke carries no truth

The event payload question (schema? ordering? dedup? versioning?) dissolves by
subtraction: **the poke is a content-free wake-up.** The store is the SSOT; any
payload describing "what changed" is a second representation of store state that
can lag, lie, or drift — and consumers *will* branch on it (Hyrum), turning an
advisory hint into load-bearing schema. A consumer's reaction to a poke is
always the same act: `bl sync; bl list --json; decide`. Therefore:

- **Lost poke** → corrected by the next poke or the poll-floor timeout. Costs
  latency, never correctness.
- **Duplicate / storm of pokes** → coalesce into one poll; the poll is
  idempotent. No dedup mechanism.
- **Ordering** → none needed; the poll reads current truth, not an event log.
- **Transport** → anything that can deliver "now" — FIFO, unix socket, HTTP
  POST, `kill -USR1`, a cron tick. All equivalent, all deployment policy.

(Consumers curious about *what* happened have the unified log — §6: the log is
already the event stream — and git history. The poke stays a bare byte so
nobody is tempted to trust it.)

## Position: nothing in core — three layers, each already possible

### Layer 0 (the floor, and possibly the whole fix): one blessed dispatcher loop

A reference dispatcher, tracked in-repo as contrib/documentation — not a verb,
not a binary, not core:

```sh
# dispatch.sh — the paved poll loop. Pokes (any writer to $WAKE) shorten the
# sleep; the timeout is the correctness floor. Runs one claim at a time.
mkfifo -m600 "$WAKE" 2>/dev/null || :
while :; do
  read -t "${POLL_SECS:-60}" <"$WAKE" || :        # poke OR timeout — either way, poll
  bl sync
  id=$(bl list -s ready --json | jq -r '.[0].id // empty') || continue
  [ -n "$id" ] || continue
  wt=$(bl claim "$id" --as "$AGENT_ID") || continue  # lost the race: resync & repick
  spawn_agent "$id" "$wt" &
done
```

Three disciplines are baked in rather than documented-and-hoped:

- **Same-box claim serialization is topology, not locking.** The loop is the
  only claimer against this clone, so bl-07d6-class races cannot arise. Parallel
  agents get their claims *routed through* the dispatcher (pre-claim serially,
  then spawn), not made concurrently beside it.
- **A failed claim is normal weather.** Cross-box contention surfaces as the
  claim's non-ff abort; the loop's answer is the loop itself — resync, repick.
  No retry mechanism, converge-on-retry (§14) already is one.
- **The poke is optional equipment.** Delete `$WAKE` and the loop still works at
  poll-floor latency. Severability: removing signaling deletes a FIFO, not code.

### Layer 1 (local pokes, zero new mechanism): a notify plugin

§6 already states this design in one sentence, written for metrics: *"the hook
list + this dispatch + the §7 payload IS the subscription"*
(docs/architecture.md:511). Every mutating op runs `<op>.post` plugins; a notify
plugin — ~10 lines: non-blocking write of one byte to the dispatcher's FIFO,
exit 0 — wired by config (`bl conf append close.post bl-notify`, likewise
`create`/`update`/`claim`/`unclaim`) is pure policy. Core is not touched; the
severability test passes (unwiring it deletes config lines).

Two contract points the reference implementation must honor:

- **Fail-open, unconditionally.** A `post` plugin's non-zero exit aborts the op
  and rolls the seal back (§8) — a notify plugin that errors because no reader
  holds the FIFO would abort a *completed claim* to report that a latency hint
  went undelivered. Never: `O_NONBLOCK`, swallow `ENXIO`/`EPIPE`, exit 0 always.
- **Wire it last.** If an earlier plugin's failure rolls the op back after the
  poke fired, the spurious poke is harmless (the poll sees no change and sleeps)
  — but last-in-list means the poke usually reports only landed ops.

Scope honesty: this layer sees only ops made *from this clone* — which is
exactly the mass-execution case that hurts today (agent A's close unblocks a
ball; the same-box dispatcher learns at the next poll instead of instantly).

### Layer 2 (remote pokes, off every agent box): the hub hook

Remote-originated changes reach a clone only at sync, so remote eventing has
exactly one honest origin: the hub — the one place every clone's push already
lands. A `post-receive` hook on a bare hub (or a forge webhook on the store
branch) pokes the dispatcher. Two properties keep §0 intact:

- **balls grows no resident process.** The hub is already a hosted git remote;
  hooking it adds no new residency to the balls model. The receiving end is the
  dispatcher — which is *inherently* resident (that is what a dispatcher is),
  and it is harness territory, not balls. Residency lives with the party that
  already has it.
- **It is droppable.** A GitHub hub with no reachable endpoint, a hub you can't
  hook, no hub at all (stealth) — all degrade to the Layer-0 poll floor. The
  hook is configuration on infrastructure balls doesn't own; core cannot even
  see it.

Multiple dispatcher boxes need nothing more: each polls + takes hub pokes, and
cross-box claim contention is already the push CAS.

## Attacked and rejected

- **A socket/daemon in core.** Forks the core bet outright (§0: no server; base
  balls is a pure local task list). Also mechanically inferior: a listening
  socket needs an owner process, a lifecycle, a registry of who listens per
  clone, stale-socket cleanup — a pile of mechanism whose entire yield is what a
  FIFO write from an op.post plugin already does as config.
- **`bl watch` (blocking verb, internally polling).** A new verb is a smell
  (§0); a blocking op forks the §8 op model; and since core has no push channel
  to block *on*, watch would just be the poll loop wearing a verb — packaging
  that costs an op-model exception and saves five lines of shell. The bl-587f
  bar (mechanism needs a consumer) cuts the other way here: the consumer needs
  the *loop*, which is contrib's to ship.
- **Event payloads.** See the invariant above — a payload is a second source of
  truth and a schema to version, bought with nothing, since every consumer's
  next act is the same poll.
- **In-core flock around the store checkout (the Q4 candidate).** Rejected on
  three counts. (a) To make the loser *succeed* rather than fail-clean, the lock
  must span the whole op — worktree-open through plugin chain — and `close.pre`
  runs the repo's own pre-commit hook (about a minute in this repo): a
  minute-scale global lock on every same-box op. (b) Plugins legitimately shell
  back into `bl` (§6: the forge gate-child mint at claim); a nested op would
  deadlock on a non-reentrant lock, and reentrancy would have to ride the
  environment — which §6 already declares plugin-controlled and untrustable
  (docs/architecture.md:615). (c) It subtracts no discipline: remote contention
  keeps the identical fail-and-rerun path regardless, so the flock would fix
  half of one failure mode while its handling stays. Post-bl-07d6 the local
  loser fails clean and rerun converges — local and remote contention now share
  ONE story (non-ff abort → rerun), and uniformity argues for leaving it. The
  dispatcher dissolves the race anyway, by topology (one claimer per clone).

## The four questions, answered

1. **Which layer owns wake-ups?** The dispatcher consumes; the notify plugin
   (local) and hub hook (remote) produce; the poll timeout guarantees. Each
   producer is optional; the floor is not. Composition, with poll as bedrock.
2. **Event payload?** None. A content-free poke; `sync` + `list` is the read.
3. **Does anything justify touching core?** No. Both poke sources exist today as
   config + infrastructure; core already exposes the subscription (§6) and the
   machine read (`list --json`). Zero core commits proposed.
4. **Same-box serialization?** Dispatcher topology (route claims through the one
   loop), documented as the paved path. flock rejected (above). bl-07d6's
   clean-fail is the correct residual: rare, loud, converges on rerun.

## If this holds: follow-up work (no balls minted pre-convergence)

- `contrib/dispatch.sh` — the reference loop above, hardened (trap/cleanup,
  jq-less fallback), <300 lines. Alternative home: a satellite repo, the
  adversary-plugin precedent — dialogue question (b).
- `contrib/bl-notify` or a documented pattern in SKILL.md — dialogue question (a).
- A SKILL.md paragraph blessing the loop ("dispatching agents? start here").
- One §15 entry pointing at this doc.

## Open questions for the dialogue

(a) Is bl-notify worth shipping as a reference binary (like bl-chore: shipped ≠
scheduled), or does a documented 10-line pattern serve better? A shipped binary
invites wiring; a pattern keeps the surface at zero.
(b) Does the dispatcher live in-repo (`contrib/`) or as a satellite repo? In-repo
is discoverable; satellite keeps core's tree free of harness opinion
(completion-gate precedent says policy satellites work).
(c) Poll-floor default: is 60s the right sleep for the reference loop, or should
the reference refuse to pick a number and force the deployer to?
(d) Is there any appetite for the poke to carry an opaque hint line (op + id,
explicitly non-contractual) for humans tailing the FIFO — or does that invite
the Hyrum consumer this doc just banned? (Position: banned; the log already
answers "what happened".)
