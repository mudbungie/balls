# bl-9df0 — stealth.lock is write-only: consent withheld doesn't bind the next op

**CONVERGED 2026-06-10 (maintainer dialogue): Option A, implemented.** The
maintainer's framing: stealth is implicitly "the remote is the landing/local
tasks directory" — once set, unless you install a remote configuration, you
never get one. The authoritative record is the §15 entry in
`docs/architecture.md`; this file preserves the dialogue's option analysis.
Sub-answers: (1) `--stealth` writes config on an established landing too — an
explicit flag you typed is the §4 "by you" path; (2) the sentinel TRAVELS on
`install` — checkout policy, while URLs stay per-machine (a URL in the landing
rung is refused); (3) loud post-miss pushes accepted — the silent degrade stays
founding-prime's alone.

## The invariant being protected

**A consent opt-out must bind every store-touching op, not just the verb that
declared it.** §12's promise — "`--stealth` opts out (and locks the store
local)" — is a statement about the *checkout*, but the implementation realizes
it as a statement about *one prime invocation*. `bl prime --stealth` sets
`binding.stealth` (argv → `bind()`, `src/checkout.rs`), the tracker's
`effective_remote` resolves no remote, prime writes `stealth.lock` into the
clone bundle — and stops. Nothing ever reads the lock back. The next mutate op
(`bl create`) binds with `stealth: false`, `effective_remote` rediscovers
`origin`, and its `*/post` push **implicitly founds `balls/tasks` on origin** —
the exact act the flag exists to refuse. (It also falsifies `remote_ops::push`'s
own doc: "always to an ESTABLISHED store — founding is prime's alone".)

## Ground truth

- `stealth.lock` has **two writers, three meanings**, all in `src/tracker/prime.rs`:
  1. `prime/pre` with no remote (`prime()` line 40) — fires for *declared*
     stealth (`--stealth`) AND for *circumstantial* stealth (no origin, no
     explicit remote). Two different facts, one byte-identical file.
  2. `prime/post` founding-miss (`prime_post()` line 90) — the bootstrap push
     rejected for lack of create perm; §12 explicitly promises this is
     transient: "(Re-running `prime` re-attempts; if another clone has since
     founded the branch it is now present → adopt.)" (§12:1111).
- **Zero readers.** `grep -rn stealth.lock src/` hits only the writers and
  tests asserting the file exists.
- The lock therefore records an **outcome** ("this prime ended local"), not a
  **consent** ("this checkout declined to publish"). Outcomes are derivable
  per-op (does origin exist? does the remote branch exist?) — persisting them
  is a derive-don't-store violation (§0). Consent is the only fact here that
  is *not* derivable, and it is the one fact we fail to persist anywhere
  readable.

This is why the "obvious" fix — make the lock gate the implicit-origin tier for
all ops — collides with §12:1111: the founding-miss writer would turn a
transient miss into a permanent opt-out, breaking the re-prime promise. The
collision is not between two requirements; it is between two unrelated facts
sharing one file.

## Candidates

### A. Maximal subtraction — delete the lock; stealth is a value in the ONE ladder

§12 already states the right frame: *"point it local and you are stealth"* —
stealth **is a config fact**, federation's zero case, not a parallel mechanism.
The bug exists because that fact has no durable representation: `tasks_branch`
is just a name; locality is decided by remote resolution; and the resolution
ladder has no rung that can say "nothing, on purpose".

So: give the §12 ladder a per-checkout durable rung that can hold the sentinel
`none`, and delete `stealth.lock` entirely (both writers).

```
--remote/--center (per-op, argv)              # consent given supersedes withheld — for this op
  > landing task-remote (per-checkout, §4)    # "none" = declared stealth: resolution STOPS here
    > XDG task-remote (per-machine)
      > discovered origin (per-repo)
```

- `bl prime --stealth` becomes sugar for writing `task-remote = "none"` into
  the landing config (shaping the seed on a founding prime; an ordinary
  `bl conf set` on an established one). The flag survives because the opt-out
  must precede the first founding push and `conf` needs a landing to write to.
- `bind()` already resolves landing config every op; `binding.stealth` is
  derived from the resolved value instead of argv-only. The wire shape, the
  `effective_remote` gate, and every tracker stealth no-op are **unchanged and
  already tested** — the fix is where the bit comes from, not what it does.
- Founding-miss persists **nothing**. §12:1111 holds by construction: no state,
  nothing to clear, re-prime re-attempts. Circumstantial stealth (no origin)
  persists nothing either — it never did anything but re-derive.
- Re-federation needs no new verb or clearing rule: per-op `--remote` outranks
  the sentinel (one op, consented), and durable re-federation is
  `bl conf set task-remote <url>` / unset — exactly bl-c2de's doctrine, *"an
  override is not a pointer write, and durability is an explicit act"*. The
  existing W2 ephemeral-remote warning already tells the user how.
- bl-d234 visibility comes free and improves: declared stealth shows up in
  `bl conf`'s provenance dump as a real config value with a layer, instead of a
  hidden state file. "Deliberately stealth" vs "nothing set" vs "no origin" are
  three distinct provenance readouts.

**What doesn't this solve / what it breaks:**
- §12 currently says *"The remote is NOT a landing field: it never travels on
  install (a remote URL is per-machine, not shared config)"*. The sentinel is
  not a URL, but the spec line needs an amendment: remote *URLs* stay out of
  the landing; the stealth *policy* is per-checkout and lives in it. Transport
  on `install` is then a feature, not a leak — a deliberately-stealth team's
  config carries its policy, with install's usual consent.
- `prime --stealth` on an *established* landing is now a config edit performed
  by prime. Doctrine says "config changes only by you or by `install`" — an
  explicit flag you typed is "by you", but it's the first verb-mediated config
  write outside `install`; the maintainer should bless or reject that reading.
  (Fallback if rejected: `--stealth` only shapes the seed; on an established
  landing it errors with "use `bl conf set task-remote none`".)
- Post-founding-miss, later ops' pushes now fail **loudly** (the branch is
  still absent, the push still rejected) instead of silently never publishing.
  That is a behavior change from "intended" §12 but arguably the correct one:
  silent-local-forever is precisely the invisible-stealth failure bl-d234
  closed. The silent degrade stays founding-prime's alone.
- Scope temptation: once the per-checkout rung exists, letting it hold real
  URLs (not just `none`) closes the known "no per-clone store remote" gap from
  the cross-tracking trial. Deliberately **out of scope** here — the sentinel
  is all this bug needs — but the rung is where that future lives, which is
  evidence it's the right rung.

### B. Reframe (a) — keep the lock, read it, gate only the implicit tier

`effective_remote` checks the clone bundle: explicit remote on the binding
(per-op flag or XDG) → use it; else lock present → `None`; else discover
origin. Only `--stealth` writes the lock (the founding-miss write is deleted —
same outcome-vs-consent argument as A, and required by §12:1111 regardless).
A later prime that resolves an explicit remote deletes the lock.

**What doesn't this solve:**
- Consent now has a **second home outside config** — a file whose *presence* is
  a config value. `bl conf`'s provenance dump (the §12 "resolution the dump
  shows is exactly the one the tracker will act on" promise) is silently wrong
  unless conf is taught to read the lock too: two readers of a bespoke format,
  the §0 two-representations drift risk made flesh.
- Precedence ambiguity it must answer by fiat: the XDG `task-remote` is
  per-*machine*; the lock is per-*checkout*. Under B's "any explicit remote
  wins", setting a machine-wide task-remote silently overrides a checkout's
  declared stealth — specificity says the checkout should win, the rule says it
  loses. Either answer is arguable, which is the smell.
- It needs invented lifecycle rules (who deletes the lock, when) that A gets
  for free from `conf set`/`unset`.
- It is A with the config value exiled to a side file. Same reads, same writes,
  one extra mechanism.

### C. Reframe (b) — two lock states: "declined" vs "miss"

The lock carries its cause; declined gates all ops, miss is cleared by the next
prime. Resolves the §12:1111 collision head-on.

**What doesn't this solve:**
- The "miss" state persists a derivable outcome — derive-don't-store says it
  shouldn't exist, and indeed it pays no rent: the only thing reading it is the
  rule that says to ignore it on re-prime. A record whose sole consumer is its
  own expiry is a no-op with extra steps.
- Inherits every attack on B (second consent home, conf blindness, lifecycle
  rules), plus a state machine: two states × (writer, reader, clearer,
  precedence vs explicit tiers) to specify and test.
- This is the "special case is a missing reframe" case: the two states are two
  *different facts* (a policy and an outcome) that never belonged in one file.
  Splitting the file's states is treating the symptom of the conflation.

## Recommendation

**A.** Delete `stealth.lock` (both writers). Stealth is the §12 ladder's
per-checkout rung holding the sentinel `none`, written by `bl prime --stealth`
as a config act, read by `bind()` into the existing `binding.stealth`, gated by
the existing `effective_remote`. Consent withheld becomes durable, visible in
`bl conf`, and binds every op through the one resolution point all ops already
share; consent given supersedes it per-op via the existing `--remote` tier and
durably via the existing `conf set`. The §12:1111 founding-retry promise holds
because the founding-miss persists nothing at all. Net mechanism: −1 state
file, −2 writers, 0 new verbs, 0 new flags, one new accepted config value and
two spec amendments (the "not a landing field" line; the `--stealth`-as-sugar
definition).

Open questions for the maintainer — ANSWERED, see the convergence note at the
top: (1) established-landing writes blessed; (2) the sentinel travels;
(3) loud post-miss pushes accepted.
