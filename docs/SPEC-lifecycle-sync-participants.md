# SPEC: Lifecycle-Sync Participant Model

Status: draft
Scope: unifies state-branch sync (bl-2148) and the plugin push/sync subsystem under one negotiation primitive. No user-facing command surface changes; new opt-in config fields and optional plugin subcommands. Backward-compatible: existing plugins and `.balls/config.json` files continue to work unchanged.

Reference: `docs/SPEC-orphan-branch-state.md`. Format and tone follow that document. Where this spec uses "must" / "shall" / "is", treat it as load-bearing and revisit the spec rather than compromising the implementation.

## 1. Motivation

Today, `bl` runs two parallel systems that are doing the same job:

1. **State-branch sync.** `bl-2148` wired the claim path to push the local state-branch commit to `origin/balls/tasks`, detect non-FF rejection, fetch, run `resolve_conflict()` over the remote view, and retry. The loop is implemented inline in `claim_sync.rs`.
2. **Plugin dispatch.** `plugin::run_plugin_push` fires every active plugin's `push` subcommand after a lifecycle transition. Failures are swallowed. Conflicts cannot be expressed. There is no retry, no merge, and no concept of "the plugin pushed back."

Both subsystems are *negotiating with a remote peer about the post-transition state of a task*. Pretending they are different has real costs: every subsequent feature has to be built twice (config inheritance, failure policy, retry budget, conflict reporting), and plugin authors have to reinvent the merge machinery the core already runs.

This spec collapses the two into one primitive — `Participant` — and specifies the contract that the git-remote sync, legacy plugins, and future native plugins all instantiate. The unification is invisible to anyone who does not opt in.

## 2. Principles

Architectural invariants. An implementation choice that violates one is out of scope; revisit the spec instead of compromising.

1. **One primitive.** There is exactly one negotiation loop in the codebase. Every remote interaction — git push of `balls/tasks`, plugin `push`, plugin `sync`, future event hooks — runs through it. Parallel subsystems that re-implement parts of the loop are a regression.
2. **Soft policy, hard primitives.** `bl-2148` was explicit: tools should not fight users. Participants give the operator declarative controls (subscriptions, requiredness, gating) without the system enforcing anything against the user's stated intent. A user with `--no-sync` against a repo that wants synced participants still ships.
3. **Different ontologies are not conflicts.** A participant declares what subset of a Task it owns authoritatively (its **field projection**). Merge composes projections. "Closed in git, open in Jira" is a clean merge because the closed-in-git status does not intersect a Jira plugin's projection.
4. **Failure policy is per-event and declarative.** The decision "is this participant required for this event?" lives in config, not code. A participant cannot self-promote to required; an operator cannot demote a required participant by editing source.
5. **Backward compatibility is observable, not internal.** The spec is free to refactor internals, change types, and reroute call sites. What it must not change is the on-disk behavior of an unmodified `.balls/config.json` and an unmodified legacy plugin: same commits, same logs, same exit codes, same timing.
6. **No new state stores.** Participants do not introduce a sidecar database, a per-participant ledger, or extra files under `.balls/`. State lives in `.balls/tasks/*.json` (the canonical Task) and `.balls/config.json` (per-event policy).

## 3. Terminology

Fixed terms. Used throughout the spec and in code comments. Do not invent synonyms.

- **Participant**: a remote-or-external system that subscribes to one or more lifecycle events. The git origin remote is a participant. Each enabled plugin is a participant. A future human-gate UI is a participant.
- **Lifecycle event**: a discrete state transition `bl` runs against a task — `claim`, `review`, `close`, `update`, and the standalone `sync` invocation. Drop is intentionally out of scope; see §16.
- **Subscription**: a (participant, event) pair declaring that the participant takes part in negotiating that event's outcome.
- **Field projection**: the subset of `Task` fields a participant owns authoritatively, plus the subset it merely reads. Disjoint projections compose. Overlapping projections require an explicit merge rule.
- **Negotiation**: the propose → detect-conflict → merge → retry loop, parameterized over a wire protocol. One negotiation per (event, participant) pair.
- **Outcome**: the value a participant returns from a successful negotiation. Carries the merged Task projection plus a `CommitPolicy` (§9) and optional structured fields the participant projected.
- **Failure policy**: per-subscription declaration of how an exhausted-retry or unreachable peer affects the lifecycle event. Three variants: required, best-effort, gating.
- **Wire protocol**: the participant-specific implementation of the negotiation primitive's hooks (propose, detect_conflict, fetch_remote_view). Git-over-SSH is one. A subprocess plugin's stdin/stdout JSON is another.

The word "side-effect" is prohibited in code and design discussion of this subsystem. Sync is a participant in the lifecycle, not a side-effect of it. If a code path ever needs the word, the abstraction has slipped.

## 4. Topology

```
                         lifecycle event (claim, review, close, update, sync)
                                         │
                          ┌──────────────┼──────────────┐
                          │              │              │
                  resolve subscribed participants from .balls/config.json
                                         │
                          ┌──────────────┼──────────────┐
                          │              │              │
                     Negotiation     Negotiation     Negotiation
                       (git)         (jira plugin)   (legacy shim)
                       │ │ │            │ │ │           │ │ │
                       └─┴─┴────────────┴─┴─┴───────────┴─┴─┘
                                         │
                              compose outcomes per CommitPolicy
                                         │
                            apply to Task / state branch
                                         │
                            evaluate failure policies
                                         │
                                event succeeds or fails
```

A lifecycle event runs zero or more negotiations. Negotiations against independent participants run independently; their outcomes compose at the end. Negotiations are not transactional across participants — a partial outcome is recoverable in the same way a half-push is (§7 of the orphan-branch spec).

## 5. The Participant contract

A participant declares (statically, at registration time):

```rust
trait Participant {
    /// Stable identifier. Used in config keys, commit message prefixes,
    /// and audit logs. Must match `^[a-z][a-z0-9_-]*$`.
    fn name(&self) -> &str;

    /// Events this participant takes part in. The set is fixed; runtime
    /// changes require a config edit, not a code path.
    fn subscriptions(&self) -> &[Event];

    /// The fields this participant owns and reads. Used by the merge
    /// composer to decide whether two participant outcomes overlap.
    fn projection(&self) -> Projection;

    /// Wire-protocol hooks the negotiation primitive drives.
    fn propose(&self, ctx: &EventCtx, view: &Task) -> Result<ProposeResult>;
    fn fetch_remote_view(&self, ctx: &EventCtx) -> Result<RemoteView>;
    fn detect_conflict(&self, propose: &ProposeResult) -> ConflictClass;
    fn retry_budget(&self) -> RetryBudget;
}
```

`Projection` is a triple: `(owns: BTreeSet<Field>, reads: BTreeSet<Field>, merge: MergeFn)`. `Field` is an enum over the canonical Task field set declared in `task.rs`; arbitrary projection over a free-form `extra` map is permitted but must declare its prefix (e.g. `external.jira.*`) so the composer can reason about disjointness.

The git-remote participant's projection covers every canonical `Task` field. Plugin participants' projections almost always cover only `external.<plugin_name>.*` plus a small set of read fields; that disjointness is what makes "closed in git, open in Jira" trivially mergeable.

`merge: MergeFn` operates on the *projected* overlap of two views. The git-remote participant reuses today's `resolve.rs::resolve_conflict`. Plugins inherit a default merge that picks the participant's view (since by §3, a participant authoritatively owns its projection); plugins that need richer merge supply their own through the native protocol (separate ball: bl-8b71).

## 6. Lifecycle events and subscriptions

The five lifecycle events:

| Event | Trigger | Default participants subscribed |
|---|---|---|
| `claim` | `bl claim` | git-remote (when `require_remote_on_claim`), all `enabled` plugins with `sync_on_change` |
| `review` | `bl review` | all `enabled` plugins with `sync_on_change` |
| `close` | `bl close` and `bl update status=closed` (unclaimed) | all `enabled` plugins with `sync_on_change` |
| `update` | `bl update` (non-closing) | all `enabled` plugins with `sync_on_change` |
| `sync`  | `bl sync`, `bl prime` | git-remote always; all `enabled` plugins (regardless of `sync_on_change`) |

The "default participants" column is the legacy mapping (§11): an unmodified `.balls/config.json` produces this set. Subscriptions per event are configurable (§10) once a participant or operator opts in.

`drop` is intentionally not a lifecycle event. Drop is a local-only release of a claim; it does not change the durable Task and so has nothing to negotiate. If a future case demands drop-side notification (e.g. notify Jira "agent walked away"), revisit this spec rather than smuggling drop in through the back door.

## 7. The negotiation primitive

```
Negotiation::run(participant, event, ctx, task) -> Result<Outcome>
```

Algorithm:

1. **Propose.** `propose(ctx, view=task)` returns a `ProposeResult` carrying the participant's intended new state (its projection of the post-event Task) and any wire-side staging it performed (e.g. a local commit on the state branch).
2. **Apply locally.** The participant's projection is composed into the working Task. If multiple subscribed participants propose disjoint projections, the composer merges them. If two participants propose overlapping projections, the composer runs the overlap merge function, which by default fails the event with a clear "two participants both claim ownership of field X" error.
3. **Push (protocol-specific).** The participant attempts to publish the proposal to its peer. For git-remote this is `git push origin balls/tasks`. For a subprocess plugin this is the `push` subcommand returning success.
4. **Detect conflict.** `detect_conflict(propose)` classifies the result: `Ok`, `Conflict`, `Unreachable`, `Other`. Git-remote returns `Conflict` on non-FF; a plugin returns `Conflict` when it emits a structured conflict report (§8).
5. **Resolve and retry.** On `Conflict`, fetch the remote view via `fetch_remote_view`, run the projection's merge over (proposed, remote_view), update the local working Task, and loop back to step 3. The loop is bounded by `retry_budget` (default 5 attempts).
6. **Outcome.** On success, return `Outcome { task, commit_policy, projected_fields }`. On exhaustion or `Unreachable` or `Other`, return the corresponding error variant; the failure policy (§9) decides what happens next.

The primitive is parameterized over the wire protocol but does not know about it. Adding a new participant adds a `Participant` implementation; it does not add a branch to the negotiation algorithm. Code that switches on the participant kind is a regression.

The git-remote participant's existing `claim_sync.rs::run_push_loop` is the reference implementation. After bl-eae4 lands, that function is replaced by an instantiation of `Negotiation` — same observable behavior, same test surface.

## 8. Conflict reporting from plugins

Subprocess plugins that opt into the participant protocol emit a structured conflict report on stderr (or a dedicated FD; see §15) when their wire returns a conflict. Schema:

```json
{
  "conflict": {
    "fields": ["status", "external.jira.assignee"],
    "remote_view": { "...projection of the Task as the remote sees it..." },
    "hint": "ticket was reassigned to bob in Jira since last sync"
  }
}
```

The negotiation primitive consumes the report, runs the projection's merge, and retries. A plugin that does not implement the participant protocol (legacy push/sync) cannot emit conflicts; the shim (§11) treats every legacy failure as `Other`, never `Conflict`. This is a deliberate restriction: legacy plugins don't get the retry-on-conflict path because they have no way to declare projection.

## 9. Failure policy

Per-subscription, declared in config. Three variants:

- **required**: an exhausted retry budget or `Unreachable` aborts the lifecycle event. The Task is rolled back to its pre-event state. Use for: git-remote on claim in a multi-swarm repo (the `bl-2148` "global ordering" case); a Jira ticket that legal requires to be in sync before close.
- **best-effort**: an exhausted retry budget or `Unreachable` warns and continues. The lifecycle event succeeds. The Task records the failure in `task.sync_status.<participant>` for later retry. Use for: most plugin pushes; the default for any subscription not otherwise declared.
- **gating**: an exhausted retry budget or `Unreachable` stages the proposal for human review (`bl sync --review`) and the lifecycle event proceeds in a "pending external" state. Use for: human-gate participants and audit flows where the operator must confirm before the proposal lands. The implementation lands with bl-a46d.

Failure policies are per-event: a plugin can be required for `close` and best-effort for `update`. The composition rule: an event's overall outcome is the strictest variant any participant returned. One required failure aborts; otherwise best-effort warnings accumulate; otherwise gating staging proceeds.

A required participant cannot be skipped except via an explicit per-invocation override (`--no-sync` and friends). The override is logged in the state-branch commit message so post-hoc audits can see which lifecycle events ran without their required participants.

## 10. Commit policy

The negotiation outcome carries a `CommitPolicy` chosen by the participant. This closes a story from the original design discussion: "ticket state should have commits / shouldn't have commits in updates" — different participants want different behavior, and today the core hardcodes one-commit-per-op with a fixed message.

Variants:

- **`Commit { message: Option<String> }`** — write a commit on the state branch, optionally with a participant-supplied message. Default if the participant returns no policy. Reproduces today's per-op commit exactly.
- **`Batch { tag: String }`** — accumulate the participant's state changes within the current event dispatch and commit once at the end of the event. Multiple participants returning `Batch` with the same `tag` coalesce into one commit referencing every batched participant. Coalescing is bounded by the lifecycle event invocation; never across CLI commands.
- **`Suppress`** — apply state to the working tree but do not commit. The next participant's `Commit` (or the core's fallback commit at end of event) picks up the change. **Disallowed for required participants**: a required outcome must be durable, and `Suppress` could lose state if no subsequent commit lands. The negotiation primitive errors at apply-time if a required participant returns `Suppress`.

Safety bounds (load-bearing):

- A participant cannot suppress *another* participant's commit. Policy is per-outcome, not global.
- Plugin-supplied commit messages are wrapped with a `plugin: <name>: ` prefix on the title; the participant's content is the body. Audit attribution survives any plugin output.
- If `Batch` and `Suppress` outcomes from disjoint participants coexist in one event, the `Batch` flush at end-of-event commits the suppressed state too. `Suppress` does not opt out of being recorded; it opts out of *causing* the commit.

The default behavior of an unmodified plugin (legacy shim) is `Commit { message: None }`, which reproduces today's `balls: update external for <id>` commit message exactly.

## 11. Config inheritance

Per-event participant policy is configured. The precedence rules from `bl-2148` extend to participants:

```
state-branch .balls/config.json   (repo default)
        ↓ overridden by
local .balls/local/config.json     (per-clone)
        ↓ overridden by
per-invocation flag                (--sync, --no-sync, --required=, --skip=)
```

Schema (additive on top of today's `PluginEntry`):

```json
{
  "plugins": {
    "jira": {
      "enabled": true,
      "config_file": ".balls/plugins/jira.json",
      "participant": {
        "subscriptions": {
          "claim": { "policy": "best-effort" },
          "review": { "policy": "best-effort" },
          "close": { "policy": "required" },
          "update": { "policy": "best-effort" },
          "sync": { "policy": "best-effort" }
        }
      }
    }
  }
}
```

The legacy `sync_on_change: bool` field maps to: `true` → subscriptions on `claim`, `review`, `close`, `update` with default policy; `false` → no event subscriptions, only the standalone `sync` event. This mapping is part of §12 and produces byte-identical observable behavior.

`require_remote_on_claim` (bl-2148) maps to: `true` → git-remote participant subscribes to `claim` with `required` policy; `false` → git-remote participant does not subscribe to `claim`. Other events behave the same as today (git-remote always subscribes to `sync`).

Per-invocation overrides:

| Flag | Effect |
|---|---|
| `--sync` | Forces git-remote to `required` on the current event. Mirrors today's `bl claim --sync`. |
| `--no-sync` | Forces git-remote subscription off for the current event. Mirrors today's `bl claim --no-sync`. |
| `--skip=NAME` | Removes participant `NAME` from the current event's subscription set. |
| `--required=NAME` | Forces participant `NAME` to `required` on the current event. |

Overrides are recorded in the state-branch commit message (e.g. `[--no-sync]`) so the audit trail shows which negotiations ran.

## 12. Backward-compat shim

The legacy `Plugin::push` and `Plugin::sync` interface (subprocess + JSON on stdin/stdout) does not change. The shim wraps each legacy plugin in a `Participant` implementation that:

- Declares `subscriptions` based on `sync_on_change` per the §11 mapping.
- Declares a `projection` covering only `external.<plugin_name>.*` (its only authoritative output today) and the full Task as read.
- Implements `propose` by invoking `balls-plugin-<name> push` with the proposed Task on stdin.
- Implements `fetch_remote_view` as a no-op returning the current Task projection (legacy plugins have no way to express remote state).
- Implements `detect_conflict` as a hard `Other` on any non-zero exit (legacy plugins cannot signal recoverable conflicts; see §8).
- Returns `CommitPolicy::Commit { message: None }` (legacy default).
- Defaults to `best-effort` failure policy (matches today's swallowed-failure behavior).

The shim is the reference for "legacy plugins observe no behavior change." Conformance test §17.1 verifies this on a corpus of recorded fixtures: pre- and post-unification runs produce byte-identical state-branch commits, exit codes, and stdout/stderr, modulo timestamps.

`run_plugin_push` and `run_plugin_sync` in `src/plugin/mod.rs` become thin wrappers that delegate to the negotiation primitive via the shim. They keep their signatures so the existing call sites in `commands/lifecycle.rs` are not touched in this ball — the call-site rewire is bl-2bf7.

## 13. Forward compatibility

The Task struct gains a `#[serde(flatten)] extra: BTreeMap<String, Value>` catch-all (bl-d31c). This is what makes mixed-version state-branch propagation safe: a newer `bl` adds a participant-shaped field to the Task; an older `bl` reads the file, round-trips the unknown field through `extra`, and writes it back unchanged. Without this, `bl-2148`'s active push of state-branch commits to remotes would silently drop newer fields on every save by an older client.

bl-d31c is a hard prerequisite for landing any participant-emitted Task field. It is filed as an independent sibling of this SPEC because it is also useful on its own and should not block on the SPEC.

## 14. Migration plan

The implementation children of bl-b7fb land in this order. Each child is observable; intermediate states are usable production code.

1. **bl-d31c** (forward-compat deserializer). Independent. Lands first because every subsequent ball risks adding a Task field.
2. **bl-26de** (this SPEC). Documents the contract before code lands.
3. **bl-eae4** (extract negotiation primitive). Pure refactor of `claim_sync.rs::run_push_loop` into `Negotiation<P: Protocol>`. Same observable behavior. The git-remote `Protocol` impl is the only one in tree at this point.
4. **bl-1ea6** (Participant trait + git-remote reference impl). Defines the trait. Reroutes the claim-side state-branch sync through `Participant` → `Negotiation`. Plugin dispatch is untouched.
5. **bl-b1dd** (legacy shim). Wraps existing subprocess plugins as Participants. After this lands, plugin push and sync flow through the negotiation primitive too. Behavior is byte-identical to today.
6. **bl-50c5** (config inheritance). Adds the §11 schema and resolution rules. Old configs load via the §11 mapping; new configs gain per-event policy.
7. **bl-2bf7** (apply to review/close). Subscribes participants to `review` and `close` events explicitly, using the failure-policy machinery from bl-50c5.
8. **bl-4e7d** (commit policy). Adds the `CommitPolicy` field to `Outcome`. Legacy shim uses `Commit { message: None }`; native plugins can opt in.
9. **bl-8b71** (native participant protocol). Adds the protocol version of plugin subcommands that lets plugins declare projections, return structured conflict reports (§8), and supply custom merge functions.
10. **bl-a46d** (human-gate participant). Adds `gating` failure-policy machinery and `bl sync --review`.

After step 5 the unified path is the default. After step 8 the unification surfaces in plugin authoring. After step 10 the human-gate flow is a participant like any other.

## 15. Diagnostics and audit

Every negotiation produces an audit record:

```
{
  "ts": "<rfc3339>",
  "event": "claim",
  "task_id": "bl-xxxx",
  "participant": "git-remote",
  "outcome": "Pushed" | "Lost" | "Conflict-resolved" | "Failed",
  "retries": 1,
  "wire_status": "..."
}
```

Audit records are written to the diagnostics channel introduced for plugins (`BALLS_DIAG_FD` in plugin/diag.rs); for git-remote and core-side participants they go to the same channel via an internal sink. They are *not* committed to the state branch — they are observability, not durable state. A consumer who wants persistent audit trails uses the existing state-branch git history (commit messages plus the override log from §11).

This is the answer to "where do I see what happened during sync?" without introducing a new state store (Principle 6).

## 16. Non-goals

- **No new plugin discovery mechanism.** Plugins are still `balls-plugin-<name>` on `PATH`. Adding a participant requires either configuring an existing plugin or writing one that follows the existing executable convention.
- **No server-side enforcement.** The `bl-2148` stance applies: soft policy, hard primitives. The system gives the operator controls; it does not enforce participant requirements against an explicit operator override.
- **No custom Status enum extensions.** The canonical Status enum stays at five variants. Remote-side state (Jira "Done", Linear "Cancelled", etc.) lives in projections under `external.<plugin_name>.*`.
- **No transactional cross-participant writes.** A claim that succeeds against git but fails against Jira is two separate outcomes, not a rolled-back transaction. The failure policy decides what the lifecycle event does about it.
- **No drop-side participation.** Drop is local-only (§6). Future cases that demand drop-side notification revisit the spec.
- **No participant ordering guarantees beyond declared dependencies.** Two independent participants on the same event run concurrently from the operator's perspective. A participant that requires another to run first declares a dependency in its config; the negotiation primitive honors declared dependencies and rejects cycles. Implicit ordering is a regression.
- **No exposure of native git participants from plugins.** A plugin cannot impersonate `git-remote`. The `name` field is checked against a reserved-names list at registration.

## 17. Conformance tests

These are the tests that must exist (and fail for the right reason) before any implementation code lands. Each bullet corresponds to at least one test.

1. **Legacy shim is byte-identical.** A recorded fixture of pre-unification runs (claim, review, close, update against a fake plugin) replayed against the post-unification code produces the same state-branch commits, exit codes, and stdout/stderr modulo timestamps.
2. **Disjoint projections compose.** A git-remote participant and a `jira` participant both subscribed to `close`; the resulting Task carries both `status=closed` and `external.jira.id=...` after one event.
3. **Overlapping projections fail loudly.** Two participants both claiming ownership of `status` at registration time fail config validation, not at runtime.
4. **Required + unreachable aborts the event.** git-remote required for `claim` with the remote unreachable: the local state-branch commit is rolled back; no worktree is created; exit non-zero with a clear message.
5. **Best-effort + unreachable warns and continues.** A best-effort plugin unreachable during `claim`: the worktree is created, the state-branch commit lands, a warning is printed, `task.sync_status.<plugin>` records the failure.
6. **Gating + unreachable stages.** A gating participant unreachable during `close`: the close commit lands, the Task records the gating-pending state, `bl sync --review` shows the staged proposal.
7. **Conflict + retry succeeds.** A conflict on the first push, clean merge via projection, second push succeeds. Negotiation returns `Outcome { retries: 1, ... }`.
8. **Conflict + retry exhaustion.** Five conflicts in a row. Required participant: event aborts. Best-effort: warns and continues. Gating: stages.
9. **Per-invocation override is logged.** `bl claim --no-sync` against a config that declares git-remote required produces a state-branch commit message containing `[--no-sync]`.
10. **CommitPolicy::Suppress on a required participant errors at apply-time.** Before any state lands.
11. **CommitPolicy::Batch coalesces.** Three participants subscribed to one event, all returning `Batch { tag: "x" }`: one commit lands referencing all three.
12. **Plugin commit message has the safety prefix.** A native participant returning `Commit { message: Some("custom") }`: the title carries `plugin: <name>: ` and the body is `custom`.
13. **Forward-compat round-trip.** A Task written by a synthetic newer-version serializer (with an unknown field) read and re-written by the current `bl` preserves the unknown field in `extra`. (Verifies bl-d31c is in place; the participant model relies on it.)
14. **Hand-edit workflow still works.** The §11 hand-edit sequence from `SPEC-orphan-branch-state.md` produces the same observable behavior post-unification: a hand edit committed to the state branch is picked up by the next sync. Participants do not require `bl` to write Task files.
15. **Reserved participant names are rejected.** A plugin named `git-remote` or `core` fails registration.
16. **Audit records are emitted but not committed.** Every negotiation writes one audit record to the diagnostics channel. No audit content appears in any state-branch commit.

No implementation change lands until every test in this list exists and fails for the right reason.

## 18. Open questions

These are explicitly unresolved. Resolve them in follow-up tasks, not by ad-hoc decisions during implementation.

1. **Per-event retry budget.** Today's claim path uses `MAX_RETRIES = 5`. Should `claim` and `close` share a budget, or should `close` get more attempts because partial-close is more disruptive than partial-claim? Defer until real exhaustion incidents show up.
2. **Audit record persistence.** §15 says audit is observability, not state. If operators ask for durable audit, the answer is probably a separate `bl audit` log on its own orphan branch, not stuffing it into `balls/tasks`. Out of scope here.
3. **Concurrent participants on one event.** §16 says they run concurrently from the operator's perspective. Whether the implementation actually parallelizes (threads, async) or runs them in declared order is a performance question, not a contract question. Pick the simplest thing first.
4. **Native participants implemented in-process.** Rust crates linking against `balls-core` could in principle register Participants without spawning a subprocess. The trait surface in §5 supports this, but the SPEC does not commit to shipping any. Decide when there is a concrete in-process participant worth writing.
5. **Cross-participant dependency declaration syntax.** §16 mentions declared dependencies but does not specify the config schema. Settle when the second participant that needs ordering shows up.
