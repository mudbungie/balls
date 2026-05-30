# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0](https://github.com/mudbungie/balls/compare/v0.4.2...v0.5.0) - 2026-05-30

### Changes

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
