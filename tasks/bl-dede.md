+++
title = "bl close exits 1 on a fully successful close: mutate_report.rs:73 panics after the delivery, seal and retire have all landed"
created = 1785373923
updated = 1785546293
claimant = "acerbity-dede"
priority = 4
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bug"]

[[blockers]]
id = "bl-f389"
on = "close"
+++
Observed in yog (`/home/mark/dev/yog`), 2026-07-29, closing ball bl-9130.

## What happened

`bl close bl-9130 --as entrance-9130` **succeeded in every respect that matters** — it squashed the work to `main`, sealed the store, and retired the ball — and then exited **1** with:

    bl-install-main: compiling + installing main in background -> /home/mark/dev/yog/target/cicd-install.log
    thread 'main' (3065472) panicked at src/mutate_report.rs:73:10:
    a mutating op always seals a bl-id trailer (§5)

Verified afterwards, independently of bl:

    $ git log --oneline -1
    1e8bcfb the BrazenConfig and LernieConfig watch roots ... [bl-9130]
    $ git diff --stat main work/bl-9130      # empty — main has the whole diff
    $ bl show bl-9130                        # status closed, retired 2026-07-29T06:27:15Z
    $ bl list | grep 9130                    # no rows (rc=1)

So: delivery landed, ball retired, exit 1.

## Why this is worse than a cosmetic panic

The exit code is the ONE in-band signal an automated caller has. Making it lie in the **success** direction is the expensive direction:

- The agent that ran the close read exit 1, reported the close had failed, and stopped. A coordinator then spent a full verification cycle (git log + bl list + a `ps` sweep for live close processes) establishing that the close had in fact landed — and reached the WRONG interim conclusion first, because every cheap signal agreed with the exit code.
- Retrying on a false failure is not harmless: the ball is already retired, so the retry hits a different error path and produces a second confusing message.
- Downstream, this trains callers to distrust `bl`'s exit status entirely and verify out-of-band against git — which is precisely the observability complaint filed as bl-8750.

## The assertion itself

`src/mutate_report.rs:73` asserts "a mutating op always seals a bl-id trailer (§5)". The panic fires AFTER the op has completed successfully, so either:

- the invariant is real and something in this path genuinely failed to seal a trailer (in which case the op should not have been reported as complete, and the retire is suspect); or
- the invariant does not hold for this path — plausibly because of what precedes it in the output: `bl-install-main` spawning a background compile/install, i.e. a delivery-plugin hook running around the reporting boundary.

The second is the more likely shape given the ball retired cleanly, but that is a guess from the outside — the panic message is the entire evidence available to a caller.

## Ask

1. Determine which of the two above it is. If the invariant does not hold here, the assertion is wrong, not the op.
2. Regardless: **a mutating op that has delivered, sealed and retired must not exit non-zero.** If a reporting-layer invariant is violated after the work is durable, that is at worst a warning on stderr — never an exit status that tells the caller their landed work did not land.

## Reproduction notes

Seen once, under concurrent close traffic (4-6 sibling `bl close` gates contending in the same store). Not known whether the concurrency is incidental or causal; the `bl-install-main` background spawn in the same output makes a race around the report boundary worth ruling out first.

Cross-ref: bl-8750 (behavioral — a close in flight is indistinguishable from one abandoned); bl-9042 (mid-gate CAS starvation, the concurrency this was observed under).