+++
title = "bl close is not atomic against a busy main: mid-gate advances abort on no-resurrection; store seal can fail after delivery landed"
created = 1784959126
updated = 1784959154
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
bl close is specified-atomic but observably is not, under parallel merge traffic. Evidence from the lernie 0.0.1 release drive (2026-07-24, ~15 concurrent agent closes, each close gate a 10-13 min tarpaulin run, load avg 25-30):

MODE 1 — mid-gate main advance aborts the close. Sequence: close folds main in, runs the repo pre-commit hook (long), main advances meanwhile (another agent's close), the close then aborts on the no-resurrection invariant. Observed 3x by one agent (Bulwark/lernie bl-ae66), also hit by others. Each retry costs a full gate re-run; under sustained traffic an unlucky close can starve indefinitely. Wanted: close re-folds main and re-runs the hook in a bounded loop, or takes a delivery ticket/lease so concurrent closes serialize instead of wasting whole gate runs.

MODE 2 — store seal failed AFTER the squash landed on main. Error: 'expected exactly one changed task file, found 0' followed by a FAILED ROLLBACK message, while main already carried the delivered squash and the ball did end up closed on a later look. Delivery (code lands) and seal (task file removed) are two acts with no atomicity between them, and the failure surface tells the operator to distrust a state that is actually fine — or worse, a rollback could half-undo a real delivery. Wanted: seal tolerant of/idempotent against an already-delivered close (absence of the task file = success, per 'absence is the record'), and no rollback path that can fire after the squash is on main.

MODE 3 (adjacent, same root) — a create with a title starting with '--' is refused by getopt unless the documented '-- TITLE' form is used; fine — but note create/close under concurrent store writers still occasionally needs the documented single retry ('store race'), which held up (retries clean, no corruption seen).

All three observed on stock bl from PATH (~/.local/bin/bl) against the lernie repo store. Concurrency level is the trigger; single-agent flows never hit any of this.