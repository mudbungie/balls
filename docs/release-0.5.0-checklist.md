# Release 0.5.0 — readiness checklist (bl-0da0)

Gate verdict: **GREEN** — releasable. Verified 2026-06-09 against main @ bb7d08fd.
Version 0.4.2 → 0.5.0 is a MINOR (design/API not frozen for a 1.0 commitment).

## Verified (green)

1. **Repo state** — GREEN.
   `bl list`: no surprise claimed/blocked release work (only this ball and the
   sibling-repo plugin ball claimed). No conflict markers anywhere in the main
   tree. Current version 0.4.2 in Cargo.toml; CHANGELOG `[Unreleased]` empty
   (release-plz-owned — not hand-curated).

2. **`make install` reproducible from clean main** — GREEN.
   Fresh worktree of main → `make install` compiled balls 0.4.2 from scratch
   and installed `bl` + `balls` symlink + `tracker` + `bl-delivery` siblings to
   `~/.local/bin`. The installed `bl` self-hosts: `bl list` and `bl show`
   against this very repo work.

3. **Migration holes (architecture §16 — renumbered from §17)** — GREEN, all
   four guards present and tested (`cargo test --test migrate`: 3/3 pass).
   Cutover already executed (bl-0802); this is a confirmation pass, no re-run.
   - *branch collision*: §16 "Branch & history" — legacy and greenfield share
     the `balls/tasks` name; script writes fresh orphan branches under
     `refs/migrate/*` (no checkout, no clobber); cutover is the runbook's
     history join + fast-forward push (bl-8660 — no force-push, legacy
     history kept in-branch; the old `balls-archive` recipe is retired).
   - *epic edges*: §16 "epic reciprocal edge" — migrator mints the per-live-child
     claim-blocker on the parent; asserted in
     `tests/migrate.rs::build_refs_transforms_a_real_legacy_store_end_to_end`.
   - *gh dup*: §16 "Plugin state" — github-issues adoption stamps `[bl-id]` on
     issue titles so re-sync joins with zero dup; skipping bounds cost at one
     dup per task (stabilizes, never runaway).
   - *claimed-guard*: §16 "Preconditions / guards" — script refuses on existing
     `balls/config` and on any claimed live task; asserted in
     `tests/migrate.rs::the_one_shot_guards_refuse_then_force_overrides`.

4. **release-plz flow** — GREEN.
   `release-plz.toml`: whitespace-anchored custom regexes derive the bump from
   a standalone bracketed minor/major token; `balls:`-prefixed commits are
   skipped in the changelog. Workflow `release-plz.yml` runs on push to main
   and opens/updates the release PR (it bumps Cargo.toml + CHANGELOG itself —
   the repo's convention since 0.3.x: every version bump is a
   `chore: release vX.Y.Z` commit from the release PR; never hand-bumped).
   Marker audit over v0.4.2..main (227 commits): exactly ONE match — the
   intentional minor marker on 5509ec20 "Persistent cross-repo task-master via
   committed master_url [bl-ffb4]". No stray self-match; nothing to
   REST-PATCH. Next release computes as **0.5.0**.

5. **Crate face** — GREEN.
   `cargo package --list`: 123 files — src/, docs/ (architecture.md included),
   CHANGELOG, README, SKILL.md, Makefile, default-config/, workflows. No
   lcov.info, no .balls, no profraw, no stray files. `cargo publish --dry-run`
   passes locally (compiles the packaged crate, aborts at upload as expected).

6. **Version bump** — handled by release-plz (see item 4): the minor marker is
   already in the window, so the release PR carries 0.5.0. No hand-bump made
   (would conflict with the release PR).

## Remaining operator steps (NOT performed — hard boundary)

1. Merge this ball to main (`bl close` does this) and let the release-plz
   workflow refresh the release PR; confirm its title/diff says **v0.5.0**.
2. If the PR body needs editing: `gh pr edit` is broken for it — use
   `gh api -X PATCH repos/mudbungie/balls/pulls/<n> -f body=...`.
3. Merge the release PR. **Gotcha:** `gh pr merge` can land it without firing
   the publish CI — if no release run starts, push an empty commit with a
   `balls:` message prefix to trigger it.
4. Watch the release-plz run: it publishes to crates.io and pushes tag v0.5.0.
5. Verify: `cargo search balls` shows 0.5.0; tag v0.5.0 exists on origin.
