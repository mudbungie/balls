+++
title = "Convert tracker/ to modern module form (tracker.rs + tracker/), split its tests to siblings"
created = 1780975672
updated = 1781034180
claimant = "Mason"
parent = "bl-72a8"
priority = 2
tags = ["cleanup"]
+++
Decomposition-convention cleanup surfaced by bl-0a81's structure review.

THE INCONSISTENCY
`reads/` and `tracker/` are the same KIND of thing — a namespace cluster of
distinct sibling units under one concern (reads/ = show/list/dep-tree + helpers;
tracker/ = git/payload/prime/remote_ops). But they are decomposed two different
ways:
- reads.rs + reads/      → modern Rust 2018 `foo.rs` + `foo/` form  (the model)
- tracker/mod.rs + tracker/ → dated Rust 2015 `mod.rs` form          (the outlier)

`tracker/mod.rs` is the ONLY mod.rs in the tree. It is ALSO the only cluster
keeping ALL its tests inline (`#[cfg(test)] mod tests { ... }`) even in large
files — mod.rs ~272 lines, prime.rs ~291 — against the repo's
`#[path="X_tests.rs"]` sibling-split idiom for large files.

NB: the flat `#[path] mod build` OVERFLOW splits (mutate_build.rs,
lifecycle_diffless.rs) are a SEPARATE, justified category (one module lifted past
the 300-line cap) — NOT part of this and out of scope here.

THE FIX (subtraction toward one canonical form)
- Rename src/tracker/mod.rs -> src/tracker.rs (keep the tracker/ subdir for the
  children: git.rs/payload.rs/prime.rs/remote_ops.rs/fixtures.rs).
- Split the inline tests out of the large tracker files (mod->tracker.rs,
  prime.rs) into `*_tests.rs` siblings via `#[path] mod tests`, matching the
  rest of the repo. Small tracker files may keep tests inline per the size rule.
- Keep reads/ as-is — it is the idiomatic model, not the thing to change.

WHY DEFERRED FROM bl-0a81
Module-path moves touch `use super::*` / `#[path]` wiring and are high-conflict
while the epic runs parallel agents on main; this wants a deliberate, solo pass.
Pure mechanical move — no behavior change; gate (clippy + 100% cov + line-len)
must stay green.