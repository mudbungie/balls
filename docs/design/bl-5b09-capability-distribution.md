# bl-5b09 — capability distribution: binaries never travel; hints, never auto-fetch

**CONVERGED (maintainer dialogue, 2026-07-04; dialogue ball bl-f338).** Converged AS
PROPOSED — the `[source]` free-text hint table in `plugins.toml`, hints decorating only
existing refusal moments (dispatch unbound error, install validation refusal, seed
prune), the two independent honesty fixes (install dangling-report, `bl conf` unbound
section), distribution left to the package manager — with ONE amendment to the hard
line: the eternal ban on a future explicit trust surface is retracted to a deferral (see
"The line" below). Maintainer's verdict (verbatim, 2026-07-04): "I'm not afraid of
giving someone a sharp knife, so I don't actually know if a -y/-trust flag is disallowed.
That said, don't need to solve it today, and your point about the package manager holding
the answer is right for now. The interface maintained by balls is just a pointer.
Answering provenance is a bigger operation." The §15 edit to `docs/architecture.md`
records the converged doctrine; the implementation is a separate ball, filed apart from
this dialogue. Original framing (Icecap, 2026-07-01): "Okay, how do we solve this?"

## The problem

`bl install` is a pure path-copy and "bin/ never travels" (§6) — deliberately: all
config is potential RCE (§0), binding validates a live protocol, a dangling schedule
entry prunes at seed / errors clean at dispatch, never code execution. The
consequence: adopting a center's capability set is high-friction. The real ecosystem
today — bl-adversary (close.pre review gate), the gh forge/issues plugins — lives in
sibling repos, hand-built, hand-placed beside `bl`, hand-wired. The MECHANISM is
right; the ACQUISITION story doesn't exist. §6 already names the gap in one clause:

> an adopted config ships a *recommendation* (a dangling `bin/` the recipient
> resolves locally), never runnable code.

The recommendation is **mute**. You adopt a center's config, `install` prints
`N added / M deleted`, and the first sign that the schedule names a binary you don't
have is a `close` aborting days later with no word on where the binary comes from.

## The line: invariant below, deferred above

The line has two parts. One is invariant and this design honors it absolutely; the
other was overstated as an eternal ban and is corrected here to a deferral.

**Invariant — nothing implicit, ever.** Hints are display-only. balls never fetches,
never parses, never executes what a `[source]` hint names; nothing crosses into a
landing except by the explicit copy `install` performs, and nothing runs without an
explicit human act. Implicit auto-fetch-build-bind — a remote naming a command that
balls then runs on its own — IS remote code execution, exactly what §0 forbids ("all
config is treated as potential RCE and crosses into a landing only by the explicit copy
`install` performs"), and is rejected **by definition**, here and forever. The shipped
design's behavior is exactly as specified below: a human reads the hint, a human runs
the acquisition command, and the `bin/<name>` adjacency stays the RCE gate.

**Deferred, not forbidden — an explicit trust surface.** An earlier draft of this
section declared a second, ETERNAL ban: "no future flag may 'run the hints' — that flag
would reconstitute auto-fetch behind one `-y`." That doctrine is **retracted**. The
maintainer is "not afraid of giving someone a sharp knife": an explicit `-y`/`--trust`
execute-the-hint surface — one a human deliberately reaches for, not sugar that runs on
its own — is NOT forbidden by definition. It is **deferred**, because it cannot be
designed honestly without first answering provenance: *what* exactly is being trusted,
and *how* it is authenticated. That is "a bigger operation" than this ball, and any
future design must START from the provenance question, not from sugar over the hints.
Until someone does that work, the package manager holds the answer and balls maintains
just the pointer. The two cases are not the same line: the implicit case is closed
forever; the explicit case is open but unbuilt, gated on a provenance design nobody has
written.

## The design

### 0. Distribution is the package manager's job — balls ships a pointer, not a pipeline

Plugins are ordinary installables: a crate (`cargo install balls-adversary`), a repo
with a Makefile (`make install` drops the binary beside `bl` — the adversary and gh
plugins today), an org artifact store, an apt package. balls adds **no** registry,
no namespace, no fetcher, no manifest — the world already has package managers and
§0's severability argues we must not become one. What balls owns is the one thing
only it knows: *which names its schedule needs that this box cannot resolve* — and,
with one new line of config, *what the center owner suggests you run to fix that*.

### 1. The hint: a `[source]` table in `plugins.toml`

```toml
[hooks]
"close.pre" = ["bl-adversary", "bl-delivery"]

[source]
bl-adversary = "cargo install balls-adversary"
gh-forge     = "git clone https://github.com/mudbungie/balls-github-plugin && make install"
```

- **Free text, displayed verbatim, never parsed, never executed.** By convention the
  command a human runs; core doesn't care. Rendered as a single line through the
  ordinary stderr/log path (control characters stripped — it is untrusted display
  text, same discipline as enveloped plugin stderr).
- **It lives in core's own file, beside the schedule that needs it.** `Hooks::parse`
  already documents that `plugins.toml` tables other than `[hooks]` round-trip
  untouched on `install` — the seam exists; `[source]` is the first occupant. One
  home per plugin name (single source of truth), authored by the same hand that
  authors the schedule (the center owner), traveling on the same copy (`bl install`
  of `config/` or `plugins.toml`).
- **Layered like everything else (§4):** the effective read merges landing + XDG
  `plugins.toml`, per-name scalar, innermost wins — the same file loads the dispatch
  already does for `[hooks]`; zero new read machinery.
- **Severable:** delete every `[source]` entry and behavior is bit-identical except
  terser errors. The capability (render a hint if present) is core; the policy (what
  to hint) is config — removing the policy deletes config, not code.

### 2. One rule: hints decorate existing refusals — they never create moments

Wherever core already refuses or reports on a plugin name it cannot resolve or bind,
it appends that name's `[source]` hint verbatim when one exists. No new verb, no new
flag. The moments enumerate themselves:

- **Dispatch, unbound name** (`src/plugin.rs` `unbound`) — today:
  `plugin bl-adversary referenced but bin/bl-adversary missing — run bl install`
  becomes:
  `plugin bl-adversary referenced but bin/bl-adversary missing — source: cargo install balls-adversary — then bl install to bind`
- **Install, validation refusal** (`resolve_and_bind`) — today:
  `install: refusing to link bl-adversary: does not speak protocol 1`
  becomes:
  `install: refusing to link bl-adversary: does not speak protocol 1 — source: cargo install balls-adversary`
  (the hint doubles as the upgrade pointer for a stale binary).
- **Seed, prune** — stays silent for the shipped-sibling case (a tracker-less test
  box never aborts and needs no advice), but a pruned name that HAS a hint (an org's
  XDG default-config naming third-party plugins) gets one stderr line:
  `seed: pruned bl-adversary (no binary beside bl) — source: cargo install balls-adversary — re-add with bl conf after acquiring`.
  Keying loudness on hint presence means the org opted in by authoring the hint.

### 3. Two honesty fixes, justified independently of hints

Both close "silent caps" — surfaces that today read as "covered everything" when
they didn't. Each is worth landing even if `[source]` is rejected:

- **`bl install` reports what it left dangling.** `bind_referenced`
  (`src/install_run.rs`) today *silently* skips a referenced name with no candidate —
  the Summary says `2 added / 0 deleted` and the surprise arrives at the next close.
  New: one stderr line per dangling name, `info` level (this is not core narrating
  its own mechanics — bl-cf39 demotes those to `debug` — it is an actionable
  incompleteness report, the same voice-family as the tracker's founding warnings):
  `install: bl-adversary referenced but not bound (no binary beside bl or on PATH) — source: cargo install balls-adversary — re-run bl install after acquiring`.
  The retry story already exists: a re-run converges on the no-op seal and just
  binds (§14).
- **`bl conf` grows an `unbound` section — the doctor surface, on the existing
  verb.** After the hook rows, one row per referenced-but-unbound name with its
  hint (or `(no source given)`); all bound ⇒ section absent (the general path with
  empty inputs, not a special case):

  ```
  close.pre     bl-adversary, bl-delivery            landing
  ...
  unbound  bl-adversary  cargo install balls-adversary
  ```

  Bound-state is derived at read (resolve each referenced name against the
  registry) — a query, not a field (§0). The hook rows themselves stay unmarked:
  the fact "unbound" has one home in the dump.

## The five questions, settled

**(1) Where the hint lives.** A `[source]` table in `plugins.toml` — core's own
file — not a field on schedule entries and not the plugin's config folder. Per-entry
fields break "names are pure text" and duplicate the hint across every list the
plugin appears in (drift); the plugin's folder (`config/plugins/<name>/`) is the
plugin's territory, which §0 bars core from reading — and a hint must be readable
precisely when the plugin is NOT there to speak for itself. Does core reading
`[source]` violate "core knows two things about a plugin: its name and its binary
path"? No — that invariant is about **dispatch inputs**, and dispatch is
bit-identical with or without hints (same refusals, different words in the message).
The hint is not knowledge about the plugin; it is the center owner's note to a
future human, threaded verbatim through a refusal that was already happening — the
same posture as §6 stdout: forwarded, never interpreted. The plugin itself never
reads `[source]`; core never reads `plugins/<name>/`. Both boundaries stand.

**(2) Surfacing moments and wording.** The decoration rule plus two honesty fixes,
exact strings above: dispatch unbound error, install validation refusal, install
dangling report (new line), conf dump `unbound` section (new section), seed-prune
note (only when a hint exists). Declined: a warning at `bl conf append` time — the
next op's dispatch error is seconds away, names the same remedy, and `conf` staying
binary-blind is a feature (§4: "never touches a binary").

**(3) Trust framing.** The hint's provenance is the installed config's provenance.
Installing a center's config is ALREADY the trust decision — the adopted schedule
names binaries you will run; a string suggesting where to get them is strictly less
powerful than the schedule it rides beside. A malicious center can hint
`curl | sh` — and could equally write it in a README; in this design balls' guarantee
is that **balls executes nothing**: the hint can ask, the human acts, and the
`bin/<name>` adjacency stays the RCE gate. Consent gates adoption (§6); the hint
adds no new authority to gate.

**(4) Versioning / protocol compat: out of scope.** Binding already validates the
live binary — it must speak the wire protocol and declare every op it is wired into
(`resolve_and_bind`, §6). That is the floor balls actually needs: "can I talk to
it," not "is it the version the center tested." Version ranges, manifests, and
compat matrices are new mechanism with no consumer (the bl-587f bar); a center that
cares pins in the hint text (`cargo install balls-adversary --version 0.3`), which
core never parses.

**(5) First-party bundling doesn't change the calculus.** Bundling (the bl-chore
precedent: ship in-crate, opt in by `bl conf` + prime's rebind finds the sibling)
reduces how often acquisition happens; it cannot be the answer, because the
ecosystem is open-ended — third parties cannot bundle into core, and the
capabilities people actually want (an LLM-calling review gate, GitHub-talking
forge/issues) drag deps, config, and release cadence that core must not absorb
(footprint demarcation; adversary's model/rubric is owner config in its own repo by
design). Bundling stays a per-capability packaging decision, orthogonal to this
design; hints are what make the unbundled world navigable.

## Rejected

- **Implicit auto-fetch-build-bind** (balls running what a remote names, on its own:
  a `bl install --fetch` that fetches without asking, a plugin marketplace verb that
  installs) — RCE by definition, the §0 hard line, rejected here and forever. NOT
  rejected, only deferred: an explicit human-driven `-y`/`--trust` surface — see "The
  line" — which awaits a provenance design, not a doctrinal ban.
- **Hint on the schedule entry** — duplicates per list, breaks names-are-pure-text.
- **Hint in the plugin's config folder** — core reading plugin territory (§0), and
  mute exactly when needed (plugin absent).
- **A `bl doctor` verb** — the conf dump already IS the read surface; a new verb is
  a smell (§0).
- **Version fields / compat manifest** — mechanism without a consumer; protocol
  validation is the real floor.
- **Bundling everything first-party** — makes core the distribution; §0
  severability says policy lives in config and capabilities live where they can be
  removed.
- **`bl conf append`-time unbound warning** — fourth surfacing site duplicating the
  dispatch error seconds later.

## Touches (when converged)

`src/hooks.rs` (parse `[source]` alongside `[hooks]`, layered), `src/plugin.rs`
(`unbound` wording), `src/install.rs`/`src/install_run.rs` (refusal wording +
dangling report), `src/seed.rs` (prune note when hinted), `src/conf.rs`/
`src/conf_resolve.rs` (`unbound` dump section), §4/§6 architecture text, SKILL.md
(the install row + plugins section), and `[source]` entries authored in the balls
center's own `plugins.toml` for bl-adversary + the gh plugins (the first real
consumers).
