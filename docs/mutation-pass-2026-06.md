# Mutation-testing pass — June 2026 (bl-f5b8)

Line coverage (100%, audited honest in bl-ae0b) proves execution, not assertion.
This pass ran cargo-mutants 27.1.0 over the hot paths; every surviving mutant is
a located proof that nothing asserted that behavior. All survivors were triaged
and KILLED with new tests — no accepted-equivalent mutants.

## Scope and counts

Files: `src/mutate*.rs`, `src/lifecycle*.rs`, `src/enforce.rs`,
`src/delivery*.rs`, `src/tracker.rs`, `src/tracker/*.rs` (the `*_tests.rs`
siblings and `tracker/fixtures.rs` are `#[cfg(test)]` — skipped by default).
Nothing in the planned scope was skipped.

| group                            | mutants | caught | missed | unviable | timeout |
|----------------------------------|--------:|-------:|-------:|---------:|--------:|
| mutate* (incl. args/build/edit)  |     117 |     96 |      6 |       10 |       5 |
| delivery* (repo/prune)           |      68 |     67 |      0 |        1 |       0 |
| tracker.rs + tracker/*           |      43 |     40 |      2 |        1 |       0 |
| lifecycle* (incl. diffless)      |      12 |     11 |      0 |        1 |       0 |
| enforce.rs                       |      10 |      8 |      1 |        1 |       0 |
| **total**                        | **250** | **222**|  **9** |   **14** |   **5** |

Timeouts are caught-by-hang (e.g. `+=` → `-=` on a parse-loop index); unviable
mutants fail to compile (mostly `Default::default()` on types without one).

## Survivor triage (9 missed, all killed)

| mutant | verdict |
|---|---|
| `enforce.rs:66` `&&` → `\|\|` in `blocked` | killed: `the_refusal_names_only_blockers_open_on_this_op` — the refusal message must exclude resolved blockers and blockers on other ops |
| `tracker.rs:107` delete arm `("prime","post")` | killed: `prime_post_dispatches_to_the_content_handler` — a tracked prime/post must found the absent remote branch, not fall to the catch-all no-op |
| `tracker.rs:109` delete arm `("sync"\|"prime"\|"install", _)` | killed: `sync_and_install_post_never_push_a_tracked_store` — sync/install post must NOT fall through to the generic post push (§6/§13: install never pushes the landing out) |
| `mutate_build.rs:149` `\|\|` → `&&` (×2) in `forbid_removals_on_create` | killed: `create_rejects_each_removal_flag_on_its_own` — each `--no-*` flag must bounce create ALONE |
| `mutate_build.rs:167/170/171/172` `\|\|` → `&&` (×4) in `shapes` | killed: `each_shaping_flag_bounces_an_occupancy_verb_on_its_own` — every field flag must trip the guard solo |

Pattern in the holes: long boolean disjunctions tested only with multiple (or
the first/last) operands true, and dispatch arms whose only prior test asserted
the no-op (stealth) path. The new tests pin each disjunct solo and each
dispatch arm by its observable side effect.

Verification: rerunning cargo-mutants on the three mutated files after the new
tests → 70 mutants, 63 caught, 7 unviable, 0 missed.

## How to rerun

```sh
cargo mutants -f 'src/mutate*.rs' -f 'src/lifecycle*.rs' -f 'src/enforce*.rs' \
  -f 'src/delivery*.rs' -f 'src/tracker.rs' -f 'src/tracker/*.rs' \
  --jobs 4 --timeout-multiplier 3
```

Survivors land in `mutants.out/missed.txt` (don't commit `mutants.out/`). Not a
gate — it recompiles per mutant (~4 min for this scope); run by hand
periodically and triage every survivor: add a killing test or record the
accepted equivalent here with a reason.
