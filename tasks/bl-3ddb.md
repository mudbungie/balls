+++
title = "error/notice catalog divergences and message polish (W1, E7, doubled abort prefix, misc)"
created = 1781067217
updated = 1781067217
parent = "bl-72a8"
priority = 3
tags = ["polish"]
+++
SS12 catalog vs actual output, plus small wording bugs found across the verification:

- W1 ("store is stealth (local), not auto-syncing") is never emitted at any log level - stealth is visible only via `bl conf`. W2 and the non-default-store warning, by contrast, match the spec verbatim.
- E7 ("plugin failed during prime, rolled back K prior") never appears; prime aborts use the generic message.
- Every plugin abort prints a stuttered double prefix: `bl: plugin X aborted the op: plugin X aborted the op (exit status: N)`.
- E5 (push rejected by an established store) surfaces raw git non-ff output without the catalog remedy "re-run after `bl sync`". E1/E4 are raw git passthrough with no named code (arguably fine; the codes appear nowhere).
- close notice grammar: `closed with 1 open children`.
- a create.pre plugin-assigned id colliding with a live id aborts correctly but with the oblique `create: expected exactly one new task file, found 0` instead of naming the collision.
- `bl import --legacy` with no legacy store present dies with raw `git ls-tree ... fatal: Not a valid object name balls/tasks` instead of a clean "no legacy store at <spec>" error.