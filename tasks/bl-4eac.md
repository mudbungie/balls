+++
title = "expose recursive project-delivery attempts without manufacturing balls tasks"
created = 1785730533
updated = 1786075113
claimant = "Profundity"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["design", "delivery"]

[[blockers]]
id = "bl-a1a4"
on = "claim"
+++
Source: yog bl-2b8c and the 2026-08-02 ruling that an alternative candidate is an ordinary delivery attempt, not a balls task kind.

Cross-store gate: do not claim this task until yog bl-2b8c closes with the authoritative project binding and release sequence. Balls cannot encode that dependency as a local blocker; bl-a1a4 is the local prerequisite.

## Gap

Balls already owns the recursive source-to-target mechanism, but its supported surface is task-shaped:

- claim/close derive source work/<id>, target, marker, and worktree from a real ball;
- delivery_bin::run exposes the hook wire, not a caller-facing attempt capability;
- the lower Repo::deliver seam is implementation detail and still performs the automatic target fold removed by bl-a1a4.

Yog and lernie need N >= 1 isolated project-delivery attempts for one obligation. Manufacturing one ball per alternative changes an OR choice into balls close-gate AND semantics and gives a rejected changed ball no honest retirement.

## Deliverable: one policy-blind delivery capability

Amend the balls architecture and expose one deep typed capability that both ordinary ball delivery and non-task attempts use:

1. Resolve an opaque project target and pin its exact commit; callers never construct worktree paths or ref names.
2. Materialize each write-capable attempt with its own source ref, index, worktree, and single-writer lease. Use a namespace distinct from work/*, which remains ball identity.
3. Deliver source to target using bl-a1a4's recursive law: the current target must already be incorporated, then gate, tagged/auditable squash, and CAS. A target move or stale source refuses; balls never reconciles it.
4. Separate worktree release from source-ref retention. Rejection changes no target and leaves the source addressable; the caller decides when retention expires, while balls performs safe explicit cleanup and crash convergence.
5. Return the exact base, source tip, target, and delivered commit identities needed for provenance without storing candidate, winner, cohort, or outcome state.
6. Cover deletion/move, bare repositories, concurrent attempts, retry after crash, stale target, gate failure, rejected retention, explicit discard, and ordinary claim/close parity.
7. Preserve library/binary parity required by yog embedding; decide the narrow supported interface in the living architecture before code.

The N = 1 ordinary ball path and N > 1 alternative paths must share this mechanism. No new bl verb, task status, blocker kind, fan/judge policy, merge queue, yog index, or duplicate project bytes.

## Ownership

Balls owns project refs, worktrees, delivery, and safe cleanup. Lernie owns agent history and binds an agent to the opaque attempt handle. Yog owns how many attempts exist, their variants, comparison, accept/reject/rework policy, and the retention decision.