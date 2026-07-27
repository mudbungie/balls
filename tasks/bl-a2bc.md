+++
title = "Flake: remote_ladder override test hits a create.pre id collision"
created = 1784699520
updated = 1785124591
claimant = "fathom"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
Seen once during the bl-b602 close gate, in `tests/remote_ladder.rs`:

    an_override_pushes_once_then_reverts_to_the_stealth_ladder ... FAILED
    create: a create.pre plugin reassigned the new task to `bl-be58`,
    which already exists — id collision, nothing sealed

It passed 3/3 standalone immediately afterwards and passed in two later full runs, so it is intermittent, not a hard break.

## Why it is worth a look rather than a shrug

The failure is not a timeout or an ordering assert — it is the id-minting path reporting a COLLISION. A `create.pre` plugin reassigned a new task to an id that already existed. That is either:

- a genuine collision window in id minting under concurrency (several test suites were running on the box at the time), which would be a real correctness bug that happens to surface in a test; or
- test isolation: two tests sharing a store, so one test's task id collides with another's.

The second is benign-ish and fixable by isolating the fixture. The first is not benign — balls now gates releases on CI going green, so an intermittent id collision both blocks releases at random and hints that concurrent `bl create` can lose work.

Worth determining which it is before deciding the fix. Start by checking whether the test shares a store or landing with any sibling test, and whether the `create.pre` reassignment path re-checks existence atomically or read-then-writes.

## Context

Found while landing bl-b602 (the prune-release-branches CI job); unrelated to that change, which is YAML-only.