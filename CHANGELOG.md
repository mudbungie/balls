# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.2](https://github.com/mudbungie/balls/compare/v0.5.1...v0.5.2) - 2026-06-16

### Changes

- Delivery regression: squash moves main via update_ref but never updates the landing checkout (phantom staged diff) [bl-22dd]

## [0.5.1](https://github.com/mudbungie/balls/compare/v0.5.0...v0.5.1) - 2026-06-11

### Changes

- Core narration is debug: demote mutating-op lifecycle records below the default threshold [bl-cf39]

## [0.5.0](https://github.com/mudbungie/balls/compare/v0.4.2...v0.5.0) - 2026-06-10

### Changes

- §16 cutover without a force-push: join legacy history, fast-forward the cutover [bl-8660]
- stealth.lock is write-only — a post-stealth mutate op rediscovers origin and founds balls/tasks anyway (§12 'locks the store local' only binds prime) [bl-9df0]
- pin install's validate-after-seal recovery loop: e2e test + skill note [bl-4edc]
- install --to <center> (publish direction) — the open bl-66e7 leg [bl-b8d6]
- install surface vs SS6: --from has no default, --bin absent [bl-4c45]
- front-door edge flags require a LIVE target (refuse nonexistent and dead ids) [bl-6b8c]
- skill note: hook-list ordering is yours - irreversibles last, prepend to post phases [bl-ce0c]
- error/notice catalog divergences and message polish (W1, E7, doubled abort prefix, misc) [bl-3ddb]
- update accepts id= as a preserved extra: shadow id key, lossy bedrock round-trip [bl-cd87]
- bl-delivery prime.post rollback exits 1 with No such file or directory on every aborted prime [bl-62eb]
- architecture.md as-built corrections: SS1 binding.toml + tracker territory, SS4 seed/log_level + XDG hooks file, SS6 table, SS7 wire [bl-a558]
- conf set <hooks-key> with an empty value writes [""] and dispatch fails with Permission denied [bl-bee0]
- skill note: non-bare delivery leaves the project working tree stale (add -A footgun) [bl-e3ad]
- fresh clone of a hub carrying a LEGACY balls/tasks: prime adopts it as the store and aborts, no converge [bl-868d]
- one malformed task file breaks list/show repo-wide; pre-plugin sibling edits seal unvalidated [bl-528c]
- tasks_branch = landing is structurally impossible: one branch cannot have two worktrees [bl-ac89]
- checkout-scoped seals carry no SS5 trailers at all [bl-1d9b]
- prime aborts on a deleted worktree dir instead of re-materializing (stale registration) [bl-b404]
- materialize never converges a repointed tasks_branch: store.exists() early return [bl-eb52]
- import --legacy deadlocks after the node seal: stdin lock re-entry starves the epic edge pass [bl-0a80]
- bl install unusable on a standard federated checkout: install.pre fetch hard-aborts when the remote lacks balls/config [bl-45fd]
- Converge-on-retry as the rule; rollback the appendix for external effects (+ already-delivered guard) [bl-c231]
- bl import verb — dissolve the migration script into the inverse of `show --json` [bl-e614]
- Delivery fold rigor: strict merge + no-resurrection path invariant [bl-a04a]
- Subtract the prime fixpoint: one pass, abort if tasks_branch moves [bl-698d]
- Pure-note no-op seal silently drops the -m narration — fail loudly instead [bl-cf93]
- SKILL note: intermediate worktree commits may --no-verify; close re-gates the folded tree [bl-3a95]
- --subtask-of sugar (parent + close-gate in one word) + close-time open-children notice [bl-788e]
- stdout single-writer is a default-schedule guarantee, not a discipline; reserve the enveloped-stdout seam [bl-2bff]
- Spec drift sweep #2: propagate logged §15 decisions the body missed (stdout channel, unclaim, header authority, hook dirs, union, E6, prime's branch prune) [bl-6672]
- Strike the unbuilt pull half of 'pull → mutate → push': mutating ops are optimistic, the ff-push reject (E5) IS the contention check [bl-336a]
- Forge review is a subtask, not a delivery variant: guard stays at stage, PR submission is git-native work [bl-7bfe]
- bl conf — local config read/write (scalars + plugin-schedule lists); unify the store-remote ladder to one per-op tier [bl-c2de]
- Mutation-testing pass (cargo-mutants) over the hot paths [bl-f5b8]
- Release & cutover readiness (0.5.0 bump) [bl-0da0]
- Read-op conformance: §4 narration absent on reads; list/dep-tree hook keys silently never dispatched [bl-92e8]
- bl close leaks the work/<id> branch — teardown removes the worktree, never the branch (52 accumulated) [bl-292d]
- bl sync special-cases the literal token 'landing' — §13 demands the no-op fall out of the general rule [bl-6916]
- SKILL.md asserts a nonexistent $BALLS_IDENTITY identity tier — align with the real chain (--as > $USER > unknown) [bl-48e6]
- Re-apply bl-ffaf (delete dep-tree): bl-33db's close resurrected it via a modify/delete conflict resolved the wrong way [bl-2546]
- Implement bl prime --stealth (§12): the consent opt-out the spec already promises [bl-540e]
- Bound the prime fixpoint for real — §8/§13 claim 'bounded by the §6 invocation-tree cap'; no bound exists [bl-33db]
- Delete dep-tree: its --json is a byte-level duplicate of list --json; the human forest is the only thing it owns [bl-ffaf]
- :parse lenient fallback silently no-ops a --log-level typo (warn → info) [bl-56f7]
- Split src/plugin_tests.rs (298/300) before the next edit trips the line cap [bl-44dd]
- Kill the drop verb: it is unclaim ∘ close, and it bypasses close-gates [bl-65e0]
- Prune or mark superseded the pre-freeze docs/SPEC-*.md family (cites dead paths and dead balls) [bl-2b95]
- Resolve §6 'bl plugin <name> <op> <phase>': implement the canonical-dispatch verb or strike via §15 [bl-587f]
- Pre-0.5.0 docs sweep: README + release-notes assert deleted behavior (stdout, delivery-worktree, install wiring) [bl-4098]
- Spec drift sweep: §6 hooks table vs shipped seed (3 rows), §6 prose, §12 seed-prune, §15 entries for --edit and -- [bl-3911]
- Release hygiene: .gitignore lcov.info; drop stale .balls from Cargo.toml exclude; delete wip/* tags; dedupe getrandom [bl-479d]
- bl close: unwind a post-abort delivery (sealed trailer rides every rollback) and make deliver retry-idempotent [bl-430e]
- bl close bypasses the pre-commit gate — clean delivery depends on remembering to run it by hand [bl-ee85]
- Convert tracker/ to modern module form (tracker.rs + tracker/), split its tests to siblings [bl-cc85]
- bl create/update: honor a `--` end-of-options separator so callers can pass untrusted positionals [bl-d31f]
- bl create rejects glued short flags: -p1 must be written -p 1 [bl-d165]
- Document the global --log-level in the skill guide (resolves -q/--quiet as a duplicate) [bl-3914]
- Implement §11/§15 (bl-0af4): delete delivery-worktree field + claim.pre/unclaim.pre wirings; print path on prime.post; add 'show' read-op dispatch; fix stale bl skill --json line; close ghost bl-b930 [bl-9ccb]
- bl update --edit ($EDITOR-sourced update; no new verb) [bl-e196]
- Wire the bl install verb (§6/§13) — collapses P3+P4 [bl-f387]
- Docs §6: state the invocation-tree cap is a footgun-guard, not a security boundary [bl-7dc7]
- Subtract unpopulated §7 wire field field_changes/FieldChange (P1) [bl-3bfd]
- Simultaneous claims against one local clone: loser leaves a STAGED claimant write that wedges the clone + reads as a phantom claim [bl-07d6]
- Amend §11/§15: worktree path computed-not-stored — delete delivery-worktree field; delivery compute-and-prints + read-op dispatch; plugins may run on read ops [bl-0af4]
- Harden git invocation env + arg-injection guard; depth-cap fail+rollback; bounded plugin stderr; mirror-path traversal guard [bl-2d6d] [bl-74ef]
- Standards-compliance review (clippy/semver/deps) [bl-5cf3]
- bl help: minimal command directory + expunge close-from-worktree note [bl-decc]
- Decide post-wire current_state: populate sealed after-state or strike from §7 [bl-667e]
- Fix arch-conformance gaps from bl-1a66 review [bl-abe5]
- Minimalism review — concepts, mechanism, dependency footprint [bl-004c]
- Interface ergonomics review (CLI/streams/errors) [bl-e582]
- Test-suite & coverage-honesty audit [bl-ae0b]
- adoption & §17 legacy migration (+github-issues adopt) [bl-d754]
- central/satellite federation (shared store branch) [bl-d234]
- Documentation accuracy pass (re-verify demo) [bl-b156]
- multi-agent parallel claims (worktree isolation) [bl-1b49]
- Arch & spec-conformance review (§0–§17 vs impl) [bl-1a66]
- forge-gated delivery (close.pre approval gate) [bl-53b7]
- Code structure & cleanliness review [bl-0a81]
- plugin-less / remote-less degradation [bl-9a62]
- Security review (subprocess, git, paths, secrets) [bl-2d6d]
- greenfield bootstrap & solo claim→close lifecycle [bl-7911]
- Unify status filtering under 'bl list' — fold --closed/--all into --status, add -s alias [bl-7218]
- Remove vestigial resolve_remote from core — it's a config read, not remote resolution (resolution is the tracker's) [bl-e060]
- github-issues adoption: stamp the [bl-xxxx] marker, not a per-machine join store (fix bl-2a81 federation hole) [bl-0ef9]
- Implicit origin discovery is core-side and reads the landing (local-only), not the tracker + project repo [bl-a476]
- Retire the 'dropped' status semantic — collapse to 'closed' [bl-23eb]
- §12/§15 — implicit origin discovery is the tracker's, from the project repo (not core/landing) [bl-976b]
- Greenfield prime: lazy tasks-branch materialization via core fixpoint loop (supersedes bl-fa00 reset) [bl-0a23]
- bl update: every ball field overwriteable (title/body/parent/tag-rm/extra-rm/priority-clear) — kill the create-only split [bl-9703]
- bl update can't edit a task's own blocker edges (create-only) — contradicts §16 [bl-c8b7]
- Docs + 0.x greenfield release notes + demonstration proof [bl-37b1]
- delivery code worktree mirrors the project path (SKILL + README) [bl-3d75]
- bl-delivery work worktree path (percent-encoded) breaks cargo/lld linking [bl-f3e4]
- Rewrite README.md for the greenfield model [bl-e3b3]
- Remove dead legacy balls vestiges post-cutover [bl-be65]
- Greenfield prime can't adopt an established remote store (unrelated histories) [bl-fa00]
- port 'bl skill' to greenfield + rewrite SKILL.md for the new model; fix migration runbook [bl-b1fd]
- prime --install: tracker does the remote fetch (install.pre hook); core copies local->local [bl-6e93]
- Greenfield §13: bl prime --install <center> — fuse prime + install + prime [bl-6e93]
- Greenfield §6: install becomes pure path-copy where shape decides (folder=mirror, file/glob=union); retire object enum + merge/conflict logic [bl-3dd9]
- Delivery plugin: stage worktree-path field at claim.pre/clear at unclaim.pre; recency-order delivered_in [bl-ae51]
- Rename §8 seal seam off retired discovery vocab [bl-2962]
- Seed the landing from an embedded default-config; dispatch reads config/plugins.toml [hooks] [bl-4e14]
- Implement §9 git-log-as-content reads: show history fallthrough + list --closed/--all + filters [bl-206c]
- Greenfield §11: stage derived worktree path into task frontmatter at claim.pre (authoritative, --json-readable) [bl-934a]
- prime drives sync after the prime chain [bl-8328]
- Split prime push-failure: founding-miss falls back to stealth, established-remote reject errors (§12/§15, E5) [bl-9857]
- remove the ready verb, fold into bl list --status ready|blocked|claimed [bl-6d9b]
- Delivery prime.post locates the store from cwd, not a wire binding field [bl-9773]
- Tracker §12: remote-resolution precedence, store-elsewhere warning, push-fail stealth fallback [bl-cd21]
- Bound op-log lines to PIPE_BUF for atomic concurrent appends [bl-e6a0]
- treat git log as content — recency-resolved id lookup; ready->list --ready; list history filters; show fallthrough [bl-d7a5]
- sync <branch> pulls the named branch via the binding tasks_branch override [bl-ca3f]
- §9 create emits the minted id to stdout (+ verb result lines) [bl-3b1a]
- Converge work/<id> branch derivation onto delivery::work_branch helper [bl-567a]
- Unified per-clone JSON-lines op log: pipe+envelope plugin stderr, core lifecycle records, --log-level threshold; delete dead layout::log() [bl-2e9f]
- Generalize the blocker model: on=any op + containment⊥blocking front door (§10/§15 impl) [bl-b5c0]
- Remove bl doctor code (src/doctor.rs, delivery_doctor, wiring) [bl-a38e]
- Scrub bl-20fc incidental doctor refs: retarget unwired-verb examples (dispatch/lib_tests) to install, repoint tree cycle comment to the readiness walk [bl-c541]
- Greenfield §12/§13: dissolve trail walk; sync the store branch (tasks_branch) with its remote; config never syncs [bl-9329]
- Greenfield §12/§13: dissolve trail walk; sync the store branch (tasks_branch) with its remote; config never syncs [bl-9329]
- Fix flaky tracker::prime committed-pointer test: unique fixture remote paths [bl-6a39]
- Greenfield §6: recursion guard cap -> fail+rollback (not silent plugin-free); drop the re-enable hatch [bl-7110]
- Read verbs --json is the bedrock projection: raw stored frontmatter only (literal i64 timestamps), no derived status/children/tree/ISO — aligns the impl with bl-588b §9 [bl-ae44]
- Fold bl-d074: bedrock-vs-render projection; status plugin subtracted (§3/§9/§15) [bl-588b]
- §9 read verbs: show/list/ready/dep-tree + status-ladder/ISO-8601/--json rendering [bl-20fc]
- Spec §2/§4/§6: plugins.toml hook-list replaces the filesystem symlink registry [bl-8540]
- Drop the force-rewrite prohibition from §16 migration cutover [bl-6965]
- Two-branch substrate: balls/config landing + tasks_branch store, retire operating/ [bl-f970]
- make install: per-component targets + full-suite default [bl-b30a]
- Burn §16 bl doctor out of the architecture spec [bl-77a7]
- Fold the observability resolution into docs/architecture.md (§1/§4/§6/§14/§15) [bl-b58a]
- Remove the delivery cwd-guard control (guarded bug is fixed) + soften skill doc to advisory [bl-f5b9]
- Coherence pass on the frozen architecture spec [bl-7d46]
- Remove the id_scheme config knob — one fixed scheme, custom via plugin [bl-f74d]
- Freeze greenfield architecture spec to docs/architecture.md [bl-cac0]
- Delivery plugin §16 doctor hook: code-worktree drift audit [bl-0ec3]
- §9 CLI dispatch: wire run() → mutating-verb execution [bl-35f7]
- §4 EffectiveConfig: config VALUES layer down the trail [bl-4c54]
- §11 delivery plugin prime.post worktree re-materialization [bl-3d75]
- Move blocker enforcement into core; retire the gate plugin (§10 revision) [bl-295e]
- Thread diffless before/after tip facts to post (§13) [bl-154f]
- Delivery cwd-guard: protect the agent on ALL teardown hooks (+ reliable cwd signal) [bl-4d00]
- §12/§13 prime + sync: run→engine dispatch, trail walk, pointer [bl-be5f]
- §12/§13 tracker plugin: remote sync/push, prime, trail pointer [bl-4522]
- §11 delivery/worktree plugin (direct variant): bl-delivery sibling binary [bl-f813]
- §10 blocker model: predicates, front-door reciprocals, gating plugin [bl-70f5]
- §6 bl install — committed-subtree capability transfer [bl-66e7]
- §16 base doctor — core-owned drift audit [bl-d024]
- Schema format: TOML frontmatter + unix-time timestamps [bl-651c]
- §6 subprocess plugin dispatch + §7 wire payloads [bl-5d56]
- §9 deliverable-verb base changes [bl-dfbd]
- §8 op lifecycle engine + §14 rollback [bl-4450]
- Rename anchor→landing in landed doc-comment (src/layout.rs); completes the greenfield vocab rebrand swept through spec bl-2e26 and tasks bl-66e7/bl-be5f [bl-2ddc]
- §5 commit-message trailer protocol [bl-28d2]
- §1/§2 layout substrate: encoding, XDG paths, symlink registry [bl-e755]
- §3 task schema + § id generation [bl-7b8f]
- Rewrite skeleton in place: new crate root, legacy impl deleted [bl-5c65]
- Stand up the greenfield skeleton: next module tree + §8 dispatch [bl-505f]
- Phase 1B-7: bl remaster + bl init --bare XDG-aware paths [bl-be70]
- Retire the synthesizer: route layered-field reads through EffectiveConfig [bl-c122]
- Phase 1B-5: flip cmd_init to Store::init_xdg + close §14.19 lifecycle gate [bl-213e]
- Retire the XDG state/<nested>/ catch-all (final Phase 1A SPEC §3 alignment step) [bl-a11d]
- Fold LocalConfig + tasks_dir marker into clone.json (SPEC §6.4) [bl-5a03]
- Relocate last_fetch marker to ~/.cache/balls/<nested>/ [bl-5814]
- Sunset pending_sync_legacy::warn_if_present (move diagnostic into bl doctor) [bl-341b]
- Drop per-worktree .balls/local symlink; consumers use Store XDG accessors [bl-51a5]
- Phase 1B-2: Store::init_xdg + §14.1/4/15/19 conformance (cmd_init still legacy) [bl-b273]
- bl repair --rebind-path: reverse-on-failure for partial rename failures [bl-a7dd]
- SPEC update: mark human-gate staging as deferred in SPEC-lifecycle-sync-participants [bl-f8af]
- Defer human-gate staging: keep Gating policy as a schema-accepted degrade, remove the apply/discard CLI [bl-6969]
- Refactor bl review squash to plumbing (commit-tree + update-ref), removing the detached-worktree path; 100% coverage, 889 tests pass. [bl-cb73]
- bl doctor: XDG-mode awareness for check_config + check_state_repo [bl-a4d0]
- Fix bl-05e5 moved-clone detection: invert recorded-path comparison [bl-f8f3]
- Drop seen-claim-sync-policy marker; reactive notice in bl claim [bl-1432]
- Phase 3: bl prime --migrate, bl doctor moved-clone + legacy report, bl repair --rebind-path [bl-05e5]
- Phase 2: bl migrate — pre-XDG → nested XDG conversion [bl-717e]
- Phase 1A complete: Store::discover learns XDG layout; legacy in-repo dual-read kept and warned [bl-203e]
- Land the nested-XDG layout foundation: encoding, paths, four-file config [bl-77cb]
- bl ready/claim: skip parents with live children [bl-c79c]
- clone layout — drop hashing, nest paths transparently [bl-e9c2]
- Terminology sweep: 'workspace' -> 'clone' for per-on-disk-checkout [bl-fc50]
- workspace layout — three-file model, delivery/pre_check rename [bl-dfd1]
- workspace layout — XDG dirs and orphan-branch bootstrap [bl-ed32]
- Migrate legacy committed plugin files off the code branch [bl-de57]
- Re-check task id under the per-task lock [bl-77a9]
- Terminology sweep: tracker / workspace [bl-1d46]
- Wire state_branch end to end [bl-3f59]
- Guard legacy-worktree migration against silent data loss [bl-c7b5]
- Make plugin enable agree with Plugin::resolve on config_file base [bl-1d81]
- Guard legacy-worktree migration against silent data loss [bl-c7b5]
- Commit absorbed plugin config after symlink, not before [bl-73bb]
- Split config ownership: config.json vs project.json [bl-e609]
- Resolve SPEC §16.13: new-side assertion, not old-bl fixture [bl-2b49]
- Gate non-default state_branch as unsupported [bl-022c]
- Unify the state checkout — retire federated mode [bl-8a9a]
- Collapse duplicated state-branch bootstrap and core-lib helpers [bl-af7b]
- Extract shared command/plugin-layer plumbing helpers [bl-d915]
- SPEC sweep: rewrite SPEC-tracker-state for the unified model [bl-40e9]
- Document federated multi-repo state in the README [bl-7cea]
- Fold duplicated test init_repo into git_test_support [bl-5a28]
- Decompose five files riding the 300-line cap into sibling modules [bl-c791]
- Document bl close/show delivery-resolution flags [bl-2a93]
- Sync README config schema with config.rs [bl-4ac0]
- Remove duplicated Reject header from the README [bl-0d09]
- Add SPEC-federated-state.md for the multi-repo state model [bl-81e0]
- refresh Task File Schema for all 24 task.json fields [bl-9c17]
- Discard the state-repo clone on remaster --detach [bl-692b]
- Guard the URL remaster flip against stranding local tasks [bl-20ad]
- repo field is claim-anchored, not create-anchored [bl-f80d]
- Symlink-based config symmetry: master.json pointer, gitignored canonical [bl-82a4]
- Test git helpers: scrub GIT_* via shared module [bl-d1db]
- Replace env::remove_var("PATH") in plugin tests with a thread-local seam [bl-ad4b]
- bl review: configurable pre-squash gate [bl-1f38]
- Warm bl remaster --detach: transplant state onto .balls/worktree [bl-f440]
- Anchor Task.repo at claim, not create; guard the basename fallback [bl-8994]
- correct the stale 'no repo field' multi-repo guidance [bl-4f29]
- Derive runtime_in_squash_error's help text from the path table [bl-0151]
- Kill the env::remove_var("HOME") that flaked state_repo tests [bl-bfa8]
- Federated-flip git hygiene: gitignore the .balls sidecars [bl-ebae]
- bl plugin policy/show: per-event §11 policy [bl-5cc2]
- Derive .balls runtime paths from one canonical table [bl-228d]
- delivered_repo follows the resolved repo on a remote-resolved close [bl-6816]
- detect federated mode from config [bl-4432]
- Decompose src/cli.rs under the 300-line cap [bl-8976]
- Cover .balls/state-repo in gitignore + squash backstop [bl-c439]
- unfiltered fetch fallback on filter reject [bl-dbe5]
- gitignore .balls/code-refs unconditionally [bl-0b33]
- Symlink .balls/plugins to state-repo in master_url mode [bl-1098]
- bl close: cross-repo delivered_in via delivered_repo on local miss [bl-e454]
- legacy worktree hint names the actually-corrective command [bl-82ad]
- Gitignore + runtime-paths cover .balls/code-refs [bl-c4e2]
- surface delivered_in_resolved_repo when it diverges [bl-53de]
- bl plugin enable/disable/list — hub-aware plugin management [bl-32e5]
- probe .balls/tasks symlink in master_url mode [bl-eb76]
- history fallback for closed non-gates targets [bl-3983]
- repoint stale legacy .balls/tasks symlink [bl-773e]
- transparent cross-repo resolution via delivered_repo [bl-f37b]
- collapse stale bl-88c7 reuse comment [bl-ae9a]
- branch state-checkout check on master_url [bl-c61b]
- bl close --delivered-repo: operator override for delivery provenance [bl-733e]
- Materialize .balls/tasks symlink in master_url mode [bl-38dd]
- Master wins for plugin config under master_url [bl-a7d9]
- Route sync state-leg presence gate through state_worktree_dir [bl-16e9]
- Tag delivered_in with delivered_repo provenance [bl-7523]
- Hard-fail bl prime when master_url is unreachable [bl-dcd3]
- Persistent cross-repo task-master via committed master_url [bl-ffb4]

## [0.4.2](https://github.com/mudbungie/balls/compare/v0.4.1...v0.4.2) - 2026-05-20

### Changes

- Document multi-repo operational model and tracker-proxy bridge [bl-bfb7]

## [0.4.1](https://github.com/mudbungie/balls/compare/v0.4.0...v0.4.1) - 2026-05-20

### Changes

- Anchor [minor]/[major] regex at whitespace boundaries [bl-e093]
- Suppress legacy-plugin clap 'unrecognized subcommand' on describe [bl-c343]
- Skip parent staging in close when parent is archived [bl-a04f]
- Legacy participant honors EventCtx::post for push events [bl-094b]
- Vendor minimal SHA-1; drop sha1 + hex crates [bl-cb4e]
- Auto-derive minor/major bumps from [minor]/[major] markers [bl-313a]
- document the native plugin protocol [bl-c972]

## [0.4.0](https://github.com/mudbungie/balls/compare/v0.3.11...v0.4.0) - 2026-05-20

### Changes

- Fix remaining map_unwrap_or clippy lints [bl-2fe1]
- Fix map_unwrap_or clippy lint for rust 1.95 [bl-650b]
- Wire balls to its own GitHub plugins + fix latent clippy lint [bl-71db]
- Record sha1+hex as a deliberate kept exception [bl-bd85]
- bl init --bare: first-class bare central-hub bootstrap [bl-9e8a]
- Trim chrono to default-features=false [bl-6567]
- Drop fs2 in favor of std file locking [bl-b26d]
- Document delivery modes, target_branch, and forge-gated flow [bl-5305]
- Disambiguate README SPEC cross-references to their files [bl-8e49]
- Document bootstrapping a bare central hub from scratch [bl-41ee]
- Add a bare-hub variant to the File/Folder Layout diagram [bl-82fe]
- bl close: resolve delivered_in via tag-scan when null [bl-87ea]
- Make post-delivery machinery follow per-task target_branch [bl-f788]
- Deferred-mode reject closes forge-gate child atomically [bl-3cc2]
- Document the bare central-hub repo model [bl-9f8e]
- Add advisory min_bl_version config field [bl-b7d3]
- Generous anti-DoS backstops on plugin sync ingest [bl-4673]
- Enforce participant outcomes at the command layer [bl-fb4d]
- Add per-task target_branch override [bl-d4b0]
- Deferred-squash review mode: push branch, auto-open gate [bl-b017]
- Add bl doctor: read-only repo/bl drift diagnostic [bl-2715]
- Decouple sync's state-branch leg from the code remote [bl-88c7]
- Make the integration branch explicit via target_branch [bl-0c99]
- Neutralize terminal control sequences in rendered task content [bl-b807]
- Reconstruct closed tasks from state-branch history [bl-c90c]
- Distinct, path-aware discovery errors [bl-597e]
- correct bl-2bf7 scope; point plugin-side remainder at bl-fb4d [bl-5336]
- Tolerate bare repos in Store::discover [bl-8cf7]
- Deliver EventCtx to native plugins via a describe-gated side channel [bl-bac2]
- create lifecycle event; resolve drop as observe-only participation [bl-ec62]
- First-class Reject { reason } propose outcome, wired to failure policy [bl-2062]
- Task repo provenance: record which code repo created each task [bl-499b]
- bl remaster: recover into, re-target, or detach from a shared task hub [bl-2057]
- symmetric unknown=round-trip across serde seams [bl-1b07]
- bl init safety: adopt a configured state_remote; never clobber a shared branch [bl-8e8f]
- decouple balls/tasks ref from the code remote [bl-c0c6]
- handle bl review on --no-worktree-claimed tasks [bl-7152]

## [0.3.11](https://github.com/mudbungie/balls/compare/v0.3.10...v0.3.11) - 2026-05-13

### Changes

- never deliver .balls runtime paths; rewind main on failure [bl-0dc3]

<!--
  Don't append bullets here. release-plz owns this file: each release
  cut auto-generates the new `[X.Y.Z]` section from commit subjects
  (see `release-plz.toml`; commits prefixed `balls:` are skipped).
  The rich "why" lives in commit message bodies — `bl review -m TITLE
  -m PARA -m PARA` makes those easy, and `git show <sha>` or
  `gh release view vX.Y.Z` reads them back. The CHANGELOG is the
  index; commit history is the narrative. Hand-curating bullets here
  reintroduces the dual-source drift cleaned up in bl-3751.
-->

## [0.3.10](https://github.com/mudbungie/balls/compare/v0.3.9...v0.3.10) - 2026-05-13

### Changes

- `-m`/`--message` on `bl review` and `bl close` is now repeatable, matching `git commit -m … -m …`: the first value is the commit title, each later value becomes a body paragraph separated by a blank line. Lets agents build a 50/72-shaped message inline without shell heredoc gymnastics; a single multi-line `-m` value still works exactly as before. [bl-2572]
- forge-gated delivery and configurable integration branch [bl-ac91]
- squash via detached worktree when repo root is bare [bl-56f4]
- native participant protocol [bl-8b71] [bl-8b71]
- stage plugin sync reports for review [bl-a46d]
- `bl review` and `bl close` learn the same optional remote-sync as claim: `require_remote_on_review` / `require_remote_on_close` config fields, matching `--sync` / `--no-sync` per-invocation flags, and the same precedence chain (CLI > local > repo default). Required-policy failure aborts the transition and rolls back the local commits so the task stays in its pre-transition state; close sequences the push before the worktree teardown so a rolled-back close keeps the worktree intact for retry. The git-remote participant shares the bl-eae4 negotiation primitive with claim, so a state-branch advance mid-flight (another agent's claim, etc.) auto-retries via fetch+merge. [bl-2bf7]
- commit policy on negotiation outcome [bl-4e7d]
- per-event participant policy with layered resolution [bl-50c5]
- shim legacy push/sync onto participant contract [bl-b1dd]
- trait + git-remote reference impl [bl-1ea6]
- tip on unique BALLS_IDENTITY per session [bl-90a1]
- frame worktree as the unit of work [bl-4c99]
- extract propose-merge-retry primitive [bl-eae4]
- forward-compat catch-all on Link and ArchivedChild [bl-d31c]
- lifecycle-sync participant model [bl-26de]
- `bl claim` learns optional remote-sync: with `require_remote_on_claim` set in `.balls/config.json` (or per-clone `.balls/local/config.json`, or per-invocation `--sync`/`--no-sync`), the claim commit must round-trip through `origin/balls/tasks` before the worktree is created. Closes the offline-agent claim race; off by default. Push rejects auto-resolve via the existing field-level merge — earliest-`updated_at` wins, lost claims fail loudly. `bl prime` shows a one-time hint when a clone first sees a remote-set policy. [bl-2148]
- warn that bl review auto-appends task id [bl-01d7]

## [0.3.9](https://github.com/mudbungie/balls/compare/v0.3.8...v0.3.9) - 2026-04-25

### Changes

- prime --json: route 'sync complete' to stderr [bl-f0b8]
- document JSON shape per --json command [bl-59fd]
- warn when sibling deliveries overlap your file footprint [bl-89e0]
- accept --as for identity override [bl-6f1a]
- surface main-ahead count for each claimed task [bl-23b0]
- task types: free-form identifier labels [bl-091d]
- dep tree: dotted sibling annotation on non-root rows [bl-368a]
- guide agents on when to claim gate targets [bl-dfa6]
- install `balls` symlink alongside `bl` [bl-e28f]
- --forget-half-push retracts stale half-push warnings [bl-e446]

### Changes

- `bl repair --forget-half-push <id>` and `--forget-all-half-pushes` retract stale half-push warnings via a `state: forget-half-push <id>` marker commit on the state branch [bl-e446]
- `bl dep tree` annotates non-root rows with a dotted sibling path (`.1`, `.1.2`, …) next to the id so parent chains read off at a glance without changing task ids; `--json` adds `hier_path` (omitted on roots) [bl-368a]
- task types are now free-form identifier labels: `bl create -t feature|chore|spike|…` works without code change; only `epic` still carries behavior (progress bar, `[epic]` marker); deserialize stays lenient for forward-compat [bl-091d]

## [0.3.8](https://github.com/mudbungie/balls/compare/v0.3.7...v0.3.8) - 2026-04-22

### Changes

- silence rustc 1.95 lints across 7 files [bl-0e99]
- use is_ok_and for CLICOLOR=0 check [bl-1040]
- rename backronym 'Branched' -> 'Branching' [bl-2284]
- empty squash yields delivered_in=None, skip half-push warning [bl-6729]
- add Branched Agent Labor and Logistics System backronym [bl-0512]

## [0.3.7](https://github.com/mudbungie/balls/compare/v0.3.6...v0.3.7) - 2026-04-22

### Changes

- brighten P2 priority dot [bl-e6bb]
- name the remove-task path, narrow 'deferred' [bl-ad36]
- add --limit N to cap the queue [bl-2b5a]
- reflect CLI overhaul in README and SKILL.md [bl-1eb8]
- epic bars in list/show and progress object in --json [bl-8dcf]
- header + relations + wrapped notes layout [bl-adaf]
- status column + parent hint line [bl-21a5]
- status-grouped, parent-nested layout [bl-25db]
- dep tree: parent/child rendering with box-drawing [bl-577d]

## [0.3.6](https://github.com/mudbungie/balls/compare/v0.3.5...v0.3.6) - 2026-04-18

### Changes

- render external remote_key/remote_url [bl-750c]
- Default bl flow is single-agent; review handoff is opt-in [bl-bf10]
- foundation module for CLI overhaul [bl-c784]

## [0.3.5](https://github.com/mudbungie/balls/compare/v0.3.4...v0.3.5) - 2026-04-16

### Changes

- No-git mode: bl works without a git repo [bl-e0f8]

## [0.3.4](https://github.com/mudbungie/balls/compare/v0.3.3...v0.3.4) - 2026-04-16

### Changes

- Add --tasks-dir for custom stealth task storage [bl-67ba]

## [0.3.3](https://github.com/mudbungie/balls/compare/v0.3.2...v0.3.3) - 2026-04-16

### Changes

- Cross-platform diag pipe: pipe2 on Linux, pipe+fcntl elsewhere [bl-eee4]

## [0.3.2](https://github.com/mudbungie/balls/compare/v0.3.1...v0.3.2) - 2026-04-16

### Changes

- Record per-plugin synced_at on tasks [bl-5c42]
- Add plugin diagnostics channel on FD 3 [bl-4758]
- Pass --config to auth-check and auth-setup [bl-e5d1]
- Pin uppercase-hex acceptance in validate_id [bl-9fd2]
- Wire up Config.version as a schema-version gate [bl-2da0]
- Consolidate priority validation into task::parse_priority [bl-8313]
- Forward-compat passthrough on Task and Note structs [bl-f9db]
- Lenient TaskType deserialization for forward compat [bl-fe19]
- Lenient Status deserialization for forward compat [bl-23c8]

## [0.3.1](https://github.com/mudbungie/balls/compare/v0.3.0...v0.3.1) - 2026-04-14

### Changes

- Document scripting contract in SKILL.md [bl-3662]
- release v0.3.0

## [0.3.0](https://github.com/mudbungie/balls/compare/v0.2.1...v0.3.0) - 2026-04-14

### Changes

- cli help: include gates in link subcommand doc strings [bl-fdde]
- Add gates link type for post-review close blockers [bl-c97d]
- accept bare-hex task ids without bl- prefix [bl-e501]
- add bl create row to commands table [bl-c25b]

## [0.2.1](https://github.com/mudbungie/balls/compare/v0.2.0...v0.2.1) - 2026-04-14

### Changes

- Drop 'command: release-plz' from .github/workflows/release-plz.yml. Root cause of no Release PR / no publish: the valid values for the action's command input are empty, 'release-pr', or 'release'. Setting it to 'release-plz' makes both branches of the action's shell script false, so it prints 'Using forge github' and exits in <1s without running release-plz. Removing the line makes it default to empty, which runs both release-pr and release. 2-line diff, pre-commit green. [bl-456b]
- rework rule around close-from-worktree [bl-5650]
- Replace /bin/kill shell-out with libc::killpg(pgid, SIGKILL) in src/plugin/limits.rs. Root cause of CI hangs: the kill -KILL -{pid} form parses ambiguously, errors were swallowed by 'let _', and a failed kill let the shell's sleep 20 grandchild keep the stdout pipe open, deadlocking the reader threads. libc was already an indirect dep in Cargo.lock, so adding it as a direct dep is zero build cost. All 3 plugin_limits tests pass locally in 2.1s. 100% coverage maintained, clippy clean. [bl-61b3]
- git_ensure_user on every command path [bl-534c]
- cover reviewer use-cases [bl-5650]
- Clippy pedantic cleanup [bl-548e]
- Add timeout-minutes: 6 to the CI test job. 3x the observed healthy runtime (2m0s cold-compile baseline). Prevents future hung jobs (like the bl-2b7e plugin-runner deadlock) from sitting at GitHub's 6-hour default. [bl-f998]
- Doc polish: SKILL/CLI help, Beads section rewrite [bl-941f]
- Rewrite README for orphan-branch topology [bl-1e60]
- Bump actions/checkout from v4 to v5 in ci.yml and release-plz.yml to move onto Node 24 and clear the Node 20 deprecation warning GitHub started emitting 2025-09-19. Other actions (rust-cache, install-action, release-plz) weren't flagged — leaving them until their own notices land. Zero code changes; pre-commit green (100% coverage, clippy clean, line limits clean). [bl-7ec1]
- Plugin runner: bounded output + timeout [bl-2b7e]
- clamp id_length, reject escaping worktree_dir [bl-2224]
- CI + release-plz automation for crates.io publication. [bl-8cd1]
- strict symlink check in link_shared_state [bl-06d9]
- State worktree lock for concurrent bl writes [bl-c9f1]
- make hooks target + env scrub fix [bl-e547]
- bl review: 50/72 commit message shape [bl-a03e]
- Delivery-link resolution (SPEC §6): task.delivered_in hint, tag-fallback via git log -F --grep, hint_stale detection, bl show display, json exposure. bl review sets the hint in the same state-branch commit as the review transition. 6 conformance tests cover happy path, rebase (hint stale but tag resolves), reset past tag (returns None), never-reviewed (no hint), and full review+close cycle. [bl-fc72]
- Coverage 100%, module splits, hook enforces coverage. Split src/worktree.rs -> worktree.rs (claim/drop/orphans) + review.rs (review/close/archive). Split src/commands/lifecycle.rs -> lifecycle.rs (claim/review/close/drop/update) + dep_link.rs (cmd_dep/cmd_link). Removed dead code (git_merge_no_ff, message arg, unreachable squash-conflict branch, plugin runner is_available guards duplicated by auth_check, Status::Closed print_tree arm). Consolidated sync_report per-item error handling via warn_on_err. Added tests/plugin_edge_cases.rs + tests/sync_report_edge_cases.rs covering every previously-uncovered path. Precommit hook now runs scripts/check-coverage.sh so any drop below 100% blocks commits. 297 tests pass, clippy clean, line limits clean (largest source file 278 lines). [bl-f4d4]
- Sync protocol §7.3/§7.4: state-first push order, half-push detection (warns when state close has no matching [bl-xxx] on main). Simplified auto_resolve_conflicts_at for delete/modify. Unified remove_task, git_rm_force, collapsed git_merge_inner into classify_merge, removed unused git_merge_no_ff + message arg. Coverage 96.83% -> 98.58%. All new modules 100%. Line limits clean, clippy clean. 286 tests pass. [bl-04ed]
- Orphan state branch topology: bl init creates balls/tasks + state worktree + symlink; task commits route to state branch, main log stays clean [bl-5afb]
- Text-mergeable task schema (SPEC §5). Notes moved to sibling jsonl append-only; task.json is sorted-key one-per-line. Seeded gitattributes marks notes files merge=union. New task_io module; store_paths extracted for line-limit compliance. Fixture tests cover disjoint field merge clean, same-field merge conflict, concurrent note append clean, field+note merge clean. All 275 tests pass. [bl-5308]
- Spec for orphan-branch task state. 14 sections covering principles, terminology, topology, schema invariant, delivery tag, sync protocol, stealth carveout, bl init, hand-edit workflow, non-goals, open questions, and 15 conformance tests. No code changes. [bl-030e]
- Add bl completions --install/--uninstall, refactor Makefile, document crates.io path [bl-1a34]
