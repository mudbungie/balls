+++
title = "U-balls-3: promote the delivery plugin's bin boundary as a pub lib entrypoint (delivery_bin::run)"
created = 1785131590
updated = 1785131657
claimant = "waxier-seam"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-01e5"
on = "close"
+++
Asked by yog (DESIGN §16.7 U-balls-3, yog bl-2930): a linking host that self-multiplexes bl has no sibling plugin binaries beside its exe, so it must be able to answer the bl-delivery argv/wire contract from the library. tracker already has this shape (pub tracker::run(args, stdin, stdout, &Env)); the delivery plugin's equivalent logic lived in src/bin/bl-delivery.rs main only.

DELIVERED: src/delivery_bin.rs — pub fn run(args, input, out, &Env) -> i32 carrying the whole boundary adaptation (protocol answer, wire parse, id resolution, prime housekeeping, surfaced stdout lines, the bl-delivery: error voice), pub struct Env { plugin: Option<String>, xdg: Xdg, cwd: PathBuf } per the bl-bfa8 no-env-reads-in-lib rule (plugin is Option because `protocol` is invoked bare at install time and must not need it). src/bin/bl-delivery.rs is now a thin edge over it, like bl-tracker's. Unit tests drive every arm (src/delivery_bin_tests.rs, 12 tests).

Two deliberate edge-behavior notes vs the old bin (both align the plugin with core's own Edge::resolve leniency): HOME unset no longer errors 'HOME is unset' — Xdg resolves from an empty home exactly as bl core does; an unreadable process cwd no longer aborts a prime — it degrades to '.' and the downstream git act reports. Neither is reachable under balls' own §6 spawn, which always inherits both.