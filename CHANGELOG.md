# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
