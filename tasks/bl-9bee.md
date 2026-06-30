+++
title = "Scope root_commit computation to create+claim — drop the per-op full-history rev-list walk"
created = 1782795209
updated = 1782795281
claimant = "Tightens"
priority = 4
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["core"]
+++
src/mutate.rs `dispatch` computes `Project::at(&edge.invocation_path).root_commit()` UNCONDITIONALLY for every mutating op, then injects it into `base_change`. But only `create` records it and only `claim` reads it — `update`/`unclaim`/`close` discard the value.

`Project::root_commit` runs `git rev-list --max-parents=0 HEAD`, which is a FULL-HISTORY walk: enumerating root commits means traversing every ancestor of HEAD. So every `bl close`/`update`/`unclaim` now does a needless full-graph walk in the project repo at the invocation path. Negligible for balls-sized repos, but a latent per-op cost that scales with history — and a redundant path (compute-then-discard) the 'no redundant paths' principle flags.

FIX (one line, no behavior change): compute the root only for the two verbs that use it —
    let root = match verb {
        Verb::Create | Verb::Claim => Project::at(&edge.invocation_path).root_commit(),
        _ => None,
    };
The discarded value was already None-equivalent for the other verbs, so output is identical.

Still 100%-coverable: lib_tests' create_claim_update_close round-trip exercises the `_ => None` arm (update + close) and the Create|Claim arm (create + claim), so no coverage regression.

CONTEXT: bl-1ce7 (main `41bb55e4`) shipped the unconditional form, justified by 'simpler coverage' — but that rationale was weak (the scoped form is equally coverable). This is the tightening. Per docs/design/bl-c3c0-lagging-clone.md Fix 2.