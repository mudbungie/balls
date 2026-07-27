+++
title = "U-balls-3: promote the delivery plugin's bin boundary as a pub lib entrypoint (delivery_bin::run)"
created = 1785131590
updated = 1785131590
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
Asked by yog (DESIGN §16.7 U-balls-3, yog bl-2930): a linking host that self-multiplexes bl has no sibling plugin binaries beside its exe, so it must be able to answer the bl-delivery argv/wire contract from the library. tracker already has this shape (pub tracker::run(args, stdin, stdout, &Env)); the delivery plugin's equivalent logic lives in src/bin/bl-delivery.rs main only.

Deliverable: a lib module (delivery_bin) with pub fn run(args, input, out, &Env) -> i32 carrying the whole boundary adaptation (protocol answer, wire parse, id resolution, prime housekeeping, surfaced stdout lines, the bl-delivery: error voice), Env carrying the host-resolved inputs (BALLS_PLUGIN_NAME, Xdg, cwd) per the bl-bfa8 no-env-reads-in-lib rule. src/bin/bl-delivery.rs becomes a thin edge over it, like bl-tracker's. No behavior change to the shipped binary.