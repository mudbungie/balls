+++
title = "Mutation-testing pass (cargo-mutants) over the hot paths"
created = 1781036067
updated = 1781041435
claimant = "Redress"
parent = "bl-72a8"
priority = 2
tags = ["review"]
+++
## Why

The bl-ae0b audit confirmed an honest 100% LINE coverage (2267/2267), but line
coverage proves only *execution*, not that an assertion would *catch a regression*
— the assertion-hollowness the audit had to hunt by eye (found one cosmetic case
at `prime.rs:43`). Mutation testing is the principled measure of that.

## Scope

Run `cargo-mutants` over the hot paths (`mutate.rs`, `lifecycle.rs`, `enforce.rs`,
`delivery_repo.rs`, `tracker/*`). Each surviving mutant is a located proof that
nothing asserts that behavior despite 100% line coverage. Triage survivors: add a
killing test, or record it as an accepted equivalent mutant with a reason.
**Do NOT gate on it** (slow — recompiles per mutant); this is a periodic deepening
of the suite, run by hand. Deliverable: survivor triage + tests added for the
real holes.