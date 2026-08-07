# bl-8750 — close legibility: motion is a fact about a process, not about the store

**OPEN — first draft, awaiting attack.** Stated subtraction-first: the
maximal-removed position is that balls adds *nothing*, because every question
the complaint asks is already derivable; each rung above it has to argue its way
back in. Nothing here is converged. Ledger at the bottom separates what the code
already settles from what the maintainer still has to call.

Read `docs/design/bl-1e98-claim-liveness.md` first — it converged the adjacent
question ("is this claim stale?") and its answers bind here: no stored claim
field, no lease, no reaper in core, threshold is policy, machine-locality
accepted. This ball is the *next* question, and bl-1e98 does not answer it: age
is not motion.

## The complaint, restated

An agent finishes work, starts `bl close`, backgrounds it, reports "done", and
its turn ends. The close may land or may die with its parent. What is left when
it dies — ball still `claimed`, work committed on `work/<id>`, nothing on the
target — is claimed to be **byte-identical** to a healthy close still in its
gate. Observed four times in one session; the coordinator called it right three
times and wrong once on the same evidence.

The ask, as a principle: *a close should be self-reporting, and a claim should
carry liveness* — answerable by `bl` alone, without cross-checking git.

## Ground truth — verified against main and against a live close, 2026-08-06

Three of the ball's premises do not survive contact with the code. This matters:
two of them are the reasons it asks for new mechanism.

**C1 — the `mutate_report.rs:73` panic is already fixed.** The ball's item 1
("Exit status lies") cites a panic — `"a mutating op always seals a bl-id
trailer (§5)"` — that made a fully successful close exit 1. That `.expect` is
gone: bl-dede (`a246906a`, *"bl close exits 1 on a fully successful close"*)
replaced it with the §12 warn pattern. `minted` now warns on stderr and exits 0;
`emit`'s own doc records why (*"nothing here may fail the op. Reporting runs
AFTER the seal is durable"*). **No follow-up ball is needed.** Exit status is
sound in the success direction today. The ball's suggestion that this "deserves
its own ball" is stale.

**C2 — `bl` alone already answers "did my close land?"** The ball says there is
"no `bl show <id>` field, no verb, nothing". There is: `bl show <id>` resolves a
closed id out of history and renders it, verbatim, as

```
closed   bl-1e98  Design: claim liveness — …
  status   closed
  retired  2026-07-04T20:00:45Z
```

`status closed` + a `retired` timestamp *is* the landed answer, from `bl`, one
command, no git. And the half-landed case is already documented at length —
`skill/close.md` §*"'Nothing was written' never means 'your code did not land'"*
carries the discriminator for a delivery that landed and a seal that didn't,
including that *"the note is derived from the project repo at abort time, not
guessed, so its absence is an answer."*

**C3 — the two states are not byte-identical.** A mutating op holds a change
worktree for its whole run (`clones/<enc>/changes/<uuid>/`, `layout.rs:170`),
made at Author and torn down at Teardown; a killed op cannot unwind, so it
leaves one. `converge.rs:237` already reports these as debris. The uuid does not
name the ball — but the worktree's *dirty state* does, because the base change
is written into it and not committed until Seal. Live, on this box, while this
was written:

```
$ git -C …/clones/%2Fhome%2Fmark%2Fdev%2Flernie/changes/a071a48f… status --porcelain
 D tasks/bl-3361.md
```

A deleted task file is a `close` (Retire). **The ball and the verb of every
in-flight-or-crashed op are derivable today, with zero new state.**

## The reframe — three questions, not one

The complaint reads as one question. It is three, and they live in three
different substrates. Conflating them is why it looks unanswerable.

| Question | Substrate | Answerable today? |
|---|---|---|
| Did it **land**? | the store + the target ref | Yes — `bl show <id>` (C2) |
| Is an op **in flight or crashed**, on which ball? | the filesystem (`changes/<uuid>`) | Yes — derivable (C3) |
| Is it **moving**? | a **process** | Only from the op log (Q1) |

The third is the real hole, and its shape is the whole design: **motion is not a
fact about the store.** No amount of store state can hold it, because the store
is a git branch that a dead process leaves in exactly the state a live one is
passing through. Any field that claimed otherwise — heartbeat, lease, PID —
would be a *second representation of a process's existence*, updated by that
process, and therefore wrong precisely when the process dies, which is the only
moment anyone asks. That is not a §3 "derive, don't store" nicety; it is why the
stored answer cannot work.

So the question is not *what do we store* but *what artifact already witnesses
motion*. One does.

**And the indistinguishability is not a defect — it is §1 working.** The frozen
spec's third atomicity obligation reads, verbatim: *"**(3) failure is a
NON-EVENT** — a rejected commit point leaves observable state exactly as the op
found it, so nothing needs repair before a retry."* balls *guarantees* that a
died-mid-close store is byte-identical to a never-closed one; that guarantee is
what makes "just re-run `bl close`" the whole recovery procedure. The ball is
asking for the store to distinguish two states that §1 obliges it not to. Any
in-store answer would have to weaken obligation (3), and would buy legibility
with the repair-free retry. **That trade is refused outright** — which is why
this design looks outside the store, and why "attack the principle first" was the
right instinct: the principle as literally stated ("a claim should carry
liveness") is unsatisfiable, and the satisfiable version is *an operator should
be able to ask the box*.

## Q1 — the heartbeat already exists, and it is free

`clones/<enc>/log` (`log.rs`) is the per-clone JSON-lines op log: one record per
line, `{ts, lvl, src, op, phase, msg}`. Core narration is `Debug`; a plugin's
stderr is `Info`, **enveloped line-by-line as it arrives** (`plugin_io.rs`
`capped_lines` streams; `plugin.rs:220` records each line). The delivery gate
runs the repo's `pre-commit` hook with stdout redirected onto stderr
(`delivery_repo.rs:184`), so every line the hook prints becomes a timestamped
log record **in real time**.

Measured on the busiest store on this box (`~/dev/lernie`, 368,883 records):

- **367,937 of them — 99.7% — are `bl-delivery`/`close`/`pre` gate output.** The
  log is, in practice, a gate transcript.
- Segmenting into 114 distinct gate runs: median run 90 s, and **the longest
  silence inside a run is 114 s; the median run's longest silence is 6 s.**
- Checked against a close that was live at the moment of writing: `date +%s` and
  the log's last `ts` agreed to **0 seconds**, with `bl-delivery close pre`
  confirmed by `pgrep`.

So the discriminator the ball says does not exist:

```
# is anything moving in this clone?
awk 'END{print systime() - $0}' <(tail -1 …/clones/<enc>/log | jq .ts)
```

**Silence longer than ~5 minutes is a dead close** — 2.5× headroom over the
worst observed live silence, on the box with the slowest gate. This is a *read*,
not a field: it exists because balls envelopes plugin stderr, which it does for
§6 reasons that have nothing to do with liveness.

Cost of the read: `tail -1` of one file. No index, no cache, no walk.

## Q2 — the log is an abort-only ledger, and that is backwards

`lifecycle.rs:93,98,103` emits three op-level records: `begin` (`Debug`), `seal
{sha}` (`Debug`), `abort {e}` (`Error`). The default threshold is `info`
(`log_level`, §4). Therefore **at the default level a successful op writes no
op-level record at all, and a failed one writes an Error.** Confirmed by
counting the whole lernie log:

```
194 abort records (close 180, claim 6, unclaim 3, update 3, create 2)
  0 seal records
```

An operator reading the log for a timeline sees every death and no success. For
"did it land" this is survivable (C2 answers it) — but it means the log cannot
be read as an op ledger, only as a crash log, and the natural instinct ("tail the
log, see how it ended") is defeated by the one record that would end it.

The rule the current split cites is bl-cf39: *"severity classifies the VOICE,
not the op kind"* — core narration is Debug so routine ops stay quiet. But
`abort` is **already** `Error`, so core narration is already differentiated by
*outcome*, not held uniformly at Debug. `seal` and `abort` are the same rung —
the terminal outcome of an op — currently split across two thresholds. Promoting
`seal` to `Info` is arguably a **correction of an existing asymmetry**, not a new
exception to the rule. Cost: one line per mutating op, in a file where 99.7% of
lines are tarpaulin output.

*Attack this.* The counter is that `main` and `bl show` already carry the seal,
so the log record is a second representation. The reply is that the log is a
*local timeline*, not a source of truth — it already duplicates aborts that the
terminal also printed — and a timeline missing its terminal events is not a
timeline.

## Q3 — attribution: what is derivable, and the one thing that is not

Under concurrency the heartbeat degrades. A record carries `op` (the verb token)
but no ball id and no run id, so N concurrent closes in one clone interleave
into one stream: the tail proves *a* close is alive, not *which*. Empirically
this is not hypothetical — segmenting that log into runs required guessing a
silence threshold precisely because there is no run id to segment on.

What still works under concurrency, with no new state:

- **Which balls have an op in flight or crashed**: one `changes/<uuid>` per op,
  and `git status --porcelain` in each names `tasks/<id>.md` and its verb (C3).
- **How many ops are in flight or crashed**: the count of those directories.
- **Whether the clone is moving at all**: the log tail (Q1).

What does not: *which* in-flight ball the moving stream belongs to, when more
than one is closing. With N=1 — the common case — the two reads compose into a
complete answer. With N>1 they bound it but do not resolve it.

The only fix is to **stamp the subject id on the log record** — one
`Option<&str>` in `Record`, threaded from the op, which already holds the id
before it seals (`mutate_report.rs`'s doc says so explicitly). This is the
weakest-justified rung in this doc and I flag it as such:

- *For*: it is genuinely not derivable after the fact — nothing computes it
  later, so the "don't store what you can compute" rule does not bite. The log
  already stamps `op`, `phase` and `src`; the subject is the missing member of
  that same envelope. And it is **local runtime state** — gitignored, never
  committed (`log.rs` module doc) — so it adds nothing to the store, nothing to
  the schema, and nothing that can drift between clones.
- *Against*: it is a field, and this ball was filed asking not to add one. It
  buys attribution only in the N>1 case, and N>1 is exactly the case bl-9042
  wants to make rarer.

## Q4 — the surface, subtraction-first

**Rung 0 — balls adds nothing.** All three questions are answerable today (C2,
C3, Q1). The entire cost is that the operator must know three XDG paths that
`bl` does not print. Defensible, and the maximal-removed position.

**Rung 1 — `bl conf` prints the op log and changes directory.** `bl conf`
already dumps exactly this kind of path:

```
xdg      /home/mark/.config/balls/config.toml
landing  …/clones/%2Fhome%2Fmark%2Fdev%2Fballs/config
store    …/clones/%2Fhome%2Fmark%2Fdev%2Fballs/tasks
```

Two more lines in a dump that exists. No verb, no flag, no field, no store
state, no new concept. This is what converts rung 0 from a break-in into a
documented read, and it is the smallest thing that answers the ball's actual
grievance — which is *discoverability*, not derivability. Against the maintainer's
own bl-8ab5 standard (*"a user having to bust open the tasks branch probably
implies a failure of the ergonomic surface"*), rung 1 is close to mandatory: the
log is not the tasks branch, but a path the tool never names is a break-in by any
reading. **Recommended.**

**Rung 2 — say it in `bl close --skill`.** A section that (a) names the three
questions and their reads, and (b) states the discipline rule plainly: *a `bl
close` that has not returned has not landed; do not report completion from a
backgrounded close.* Framing 3 of the ball, and it is not a cop-out — see Q5.
**Recommended.**

**Rung 3 — promote `seal` to `Info`** (Q2). Cheap, arguably a correction.
**Maintainer's call.**

**Rung 4 — stamp the subject id on the log record** (Q3). **Maintainer's call,
and the one I would drop first.**

Note what rung 2 already gets for free from Q1's `phase` field: an interrupted
close *does* report where it stopped, because every record carries its phase. A
tail ending in `phase:"pre"` died in the gate — nothing landed. A tail ending in
`phase:"post"` died after the seal — the work is on the target. That is the
ball's second framing ("make its phases legible") and it is **already true in
the schema**; it is only unexploited because nobody is told to read it. (In
practice `post` reactors are silent on success, so this discriminates aborts
better than successes — another argument for rung 3.)

## Q5 — the part no mechanism fixes

The proximate cause is not observability. It is that **the agent announced
success before the command returned.** An agent that backgrounds a 30-minute
call and reports "done" is making a claim it has no evidence for, and no surface
balls could ship would change that — the coordinator was not misled by `bl`, it
was misled by the agent.

Why agents background it: the gate is ~10 min and 3–4 attempts under contention
is normal, so a foreground close is longer than an agent turn wants to be. That
is **bl-9042's** wound (mid-gate starvation), and it is the upstream fix: every
minute the throughput work removes is a minute of pressure to detach. The two
balls share a cause without sharing a solution — bl-9042 makes the close short
enough to hold, this ball makes a detached one legible. Neither substitutes for
the other, and **the lease bl-9042 is weighing would make in-flight state legible
as a side effect**, which is a reason to sequence bl-9042 first and re-attack
rungs 3–4 afterwards rather than build them now.

## Subtractions — what this design refuses

- **No stored heartbeat, lease, TTL or PID field on a ball.** A process's
  existence recorded by that process is wrong exactly when it dies. Re-litigates
  bl-1e98 Q1.
- **No `bl status` verb.** The three questions have three existing reads; a verb
  that fans out over them is a fourth name for facts that already have three.
- **No `running` / `stale` status rung.** Status is derived from stored
  frontmatter (§3); motion is not in the frontmatter and cannot be.
- **No `--moving` / `--stale-over` list flag.** bl-8ab5's layer rule: list flags
  read STORED fields only. A threshold is policy (bl-1e98 Q2).
- **No auto-reap, no auto-kill of a silent close.** bl-1e98 Q3 settled that
  reaping is a plugin, and Q5 that a reap unclaims, never closes. Nothing here
  reopens it.
- **No cross-machine liveness.** Motion is a fact about a process on a box;
  asking another box is asking the wrong machine. bl-1e98 Q5 already accepted
  machine-locality for the same reason.

## Ledger

**Settled by the code (verify to reopen):**

1. The `mutate_report.rs:73` panic is fixed (bl-dede); exit status is sound in
   the success direction. No ball to file (C1).
2. "Did my close land?" is `bl show <id>` → `status closed` + `retired` (C2).
3. "Which ball has an op in flight or crashed?" is `changes/<uuid>` +
   `git status --porcelain` — the ball id and the verb, no new state (C3).
4. A live gate emits a log record at least every ~2 minutes (114 runs measured,
   median longest-silence 6 s); ~5 minutes of silence is a safe dead call (Q1).
5. The op log is abort-only at the default threshold: 194 aborts, 0 seals across
   368,883 records, because `seal` is `Debug` and `abort` is `Error` (Q2).
6. The store CANNOT distinguish a died-mid-close from a never-closed ball, by
   §1's third atomicity obligation ("failure is a NON-EVENT"). This is not
   negotiable and no in-store field may reopen it — the answer is outside the
   store or it does not exist.

**Open — the maintainer's calls:**

1. Rung 1 (`bl conf` prints the op log + changes paths) — recommended; is
   discoverability enough, or is a read verb wanted after all?
2. Rung 2 (`bl close --skill` carries the three reads + the foreground rule) —
   recommended; is "hold the call in the foreground" strong enough stated as
   doctrine, or does it need a refusal somewhere?
3. Rung 3 (`seal` → `Info`) — is the abort/seal asymmetry a defect to correct or
   a threshold working as designed?
4. Rung 4 (subject id on the log record) — the only non-derivable fact here.
   Worth one local field, or is N>1 attribution bl-9042's job to make rare?
5. Sequencing: does bl-9042 land first, on the argument that a delivery lease
   would make in-flight state legible as a side effect and moot rungs 3–4?
