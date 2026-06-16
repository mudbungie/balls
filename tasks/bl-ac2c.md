+++
title = "Convention: move inline mod tests to sidecars (message, verb)"
created = 1781593111
updated = 1781593113
claimant = "Sicknesses"
+++
message.rs (210) and verb.rs (207) still carry inline #[cfg(test)] mod tests, the last two over-200 files whose bulk is inline tests. Extract each to a _tests.rs sidecar (the crate's #[path] pattern), dropping source to ~130/~152. Coverage-neutral. (import.rs at 207 already uses a sidecar — it's real source, left alone.)