# Audit: test-suite & coverage honesty (bl-ae0b)

Scope: does the 100% line-coverage gate tell the truth? Verified against the tree
at commit `a5d36b9`. Baseline: **100.00%, 2267/2267 lines** (`cargo tarpaulin
--engine llvm`), genuine across every `src/` file.

## Finding 1 — the §16 migration script was outside the gate *(fixed here)*

`scripts/migrate-legacy.py` (412 lines, the legacy→greenfield base migrator) is
the largest piece of shipped behavior the gate never measured — tarpaulin sees
only the Rust crate. It ships a `--self-test`, but:

- the self-test ran **only by hand** — wired into no `make check` / pre-commit /
  `cargo test` path, so a transform regression could rot silently;
- it covers only the pure transform, and **stubs out `fold_notes`**
  (`migrate-legacy.py:332-334`), so the git-touching half — `load_legacy`,
  `fold_notes`, `guard`, `write_tree`, `build_refs`, `subtree` — and the CLI
  dispatch (`--into` / `--build-refs` / `--dry-run` / `--force`) were unexercised.

**Closed by `tests/migrate.rs`** (a Rust integration test, so it runs in every
`cargo test` gate; `tests/` is not counted by tarpaulin, so it is coverage-neutral
— verified, no `tests/` entries in the tarpaulin report). It (1) runs the script's
`--self-test`, and (2) drives `--build-refs` end-to-end against a throwaway legacy
repo (a real `balls/tasks` orphan branch of `.balls/tasks/*.json` + a
`notes.jsonl`), asserting the produced `tasks/<id>.md`: TOML-string escaping over
the git path, `type=epic`→tag, the epic reciprocal claim-edge, `fold_notes` run
for real, `depends_on`→blocker, dangling-parent nulled, `status=deferred`→tag,
closed-task file-absent, and the config branch carrying the seed verbatim. A third
test exercises both one-shot guards (`balls/config` present + a claimed live task)
and the `--force` override.

## Finding 2 — forge-gated delivery is not in core (no core gap)

Commit `2c679c4` ("Forge delivery variant + github-issues plugin port [bl-1280]")
is an **empty marker commit** on `main` — that work delivered into the sibling
plugin repo, not here. Core has no `integrate.mode`/`forge-pr`/`target_branch`
code; the only forge-relevant core surface is the generic **`gates` close-blocker**
(`enforce.rs`), and that is tested (`enforce_tests.rs`: `close_allows_a_task_with_no_gates`,
the open-gate-blocks-close cases). The forge plugin's own coverage is the sibling
repo's concern, out of scope for this gate.

## Finding 3 — federation is honestly covered; one cosmetic hollow line

Founding, adopt, clone-in, sync (fetch + ff-only), founding-miss→stealth, and
established-push-reject→E5 are each asserted in `tracker/prime.rs` and
`adopt_tests.rs`. The one line-covered-but-output-unasserted spot is the
relocated-store **warning text** at `tracker/prime.rs:43` (`eprintln!`): the
*decision* to warn is asserted directly via `store_elsewhere` (`prime.rs:258-277`);
only the message string is unchecked, and an in-process `eprintln!` is impractical
to capture at unit level. Left as-is — low severity, decision already covered.

## Finding 4 — flakiness: known fixes still in place, no regressions

- **env-race (HOME/PATH).** Zero `set_var`/`remove_var` anywhere in `src/` or
  `tests/` — tests inject `HOME`/`$XDG_STATE_HOME` per-process via `Command::env`
  (`tests/dispatch.rs` `bl_primed`) or `Xdg::with(explicit paths)`, never mutating
  process-global env. Structurally immune; bl-bfa8/bl-ad4b not reintroduced.
- **git-SHA same-second coincidence.** `reads/test_support.rs:102-108` pins
  `GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE` to a distinct per-op `@<at>` stamp, and
  the `delivered_in` ordering test relies on commit *topology* (the reused-id
  incarnation is a descendant), not timestamp luck. No equality-of-independent-
  commits assertion remains.
- **ETXTBSY exec race.** `plugin.rs:57` retains the bounded busy-retry backoff.

## Finding 5 — tarpaulin false-negatives cannot mask holes at 100%

The two known false-negatives (multi-line `Some(Struct{…})` tail-return reading
uncovered; generic-monomorphization branch hit by one instantiation) report a
*covered* line as *uncovered* — they **fail** the gate and force a fix, they
cannot silently hide an untested line. So at a green 100% they are noise, not a
masking risk. The genuine residual risk is **assertion-hollowness** (a line runs
but its effect is unasserted); the audit swept for it and found only the cosmetic
Finding-3 case — the rest of the suite asserts effects, not mere execution.

## Net

Coverage is honest. The one real hole — a 400-line transform living entirely
outside the gate — is now closed by `tests/migrate.rs` and runs under `cargo test`.
