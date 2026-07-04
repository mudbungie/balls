+++
title = "Harden gates: forbid unsafe_code + cargo audit in CI"
created = 1783111500
updated = 1783193226
claimant = "mark"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
Two additions from the 2026-07-03 repo-rectitude review (the full AI-dev best-practice list was assessed; these were the only keepers — the rest balls already has or deliberately rejects).

1. `[lints.rust] unsafe_code = "forbid"` in Cargo.toml. Zero unsafe exists in src today; forbidding it is one line that permanently deletes a bug class an agent could introduce. Pure subtraction: no new mechanism, a closed door.

2. A `cargo audit` step in .github/workflows/ci.yml — knobless RUSTSEC advisory check over the 4 runtime deps (serde, toml, serde_json, getrandom). Deliberately NOT cargo-deny: deny wants a deny.toml to police four deps, a config knob that has not earned its keep. Adopt deny only if a real license/duplicate question ever appears.

Rejected in the same review, recorded here so they don't get re-proposed: rust-toolchain pin (Cargo.toml already documents rust-version-as-floor verified by clippy incompatible_msrv); rustfmt gate (accepted format drift, edits-only style); thiserror/anyhow (std-only errors under the footprint rule); nextest/sccache/bacon (gate time is tarpaulin-dominated); unwrap_used/expect_used deny (the 100% coverage gate already forces lean error branches; retrofitting sweeps the src/ test sidecars with allows).