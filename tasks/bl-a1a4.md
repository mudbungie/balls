+++
title = "close rejects an unincorporated target: the source owner integrates before delivery"
created = 1785730516
updated = 1785824022
claimant = "Abductees"
priority = 1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["delivery", "concurrency"]
+++
Source: operator ruling during the yog project-support audit, 2026-08-02.

## Contradiction

Current delivery centralizes reconciliation inside close:

- skill/close.md says, "Delivery first folds main into your work branch";
- Project::deliver calls Project::reintegrate;
- reintegrate runs git merge --no-verify --no-edit <pinned-target>;
- the existing pinned-fold test proves a clean target advance is merged automatically.

That is not the intended scalable law. The source owner must reconcile its target before asking delivery to land. Delivery must remain a validation and atomic-advance boundary, never a merge queue.

## Invariant

For every recursive delivery edge source S -> target T:

1. Pin P = tip(T) once.
2. Require P to already be an ancestor of tip(S).
3. If not, refuse before any automatic merge, gate, squash, or target-ref move. Name S, T, and P and tell the closer to merge/rebase the target into the source worktree, resolve and test there, then retry.
4. Gate the exact source tree.
5. Mint the tagged squash with parent P and CAS T from P to that commit.

"No incoming diff" means incorporating T into S would be a no-op. T..S remains the work product.

The rule is fractal: child -> work/<parent> and root -> integration are the same operation at every depth.

## Preserve

- recursive target derivation;
- half-merge refusal and pending-work capture once the ancestry precondition passes;
- the project gate, tagged one-commit squash, pinned target SHA, no-resurrection check, and CAS;
- loud mid-gate movement refusal;
- settled/already-delivered, forge-delivered, empty, never-materialized, retry, reconciliation, and teardown behavior;
- no lease, merge queue, internal refold loop, or new verb.

## Acceptance

1. A clean disjoint target advance refuses, creates no merge commit, does not run the gate, and moves no target ref. After the closer incorporates the target, retry succeeds.
2. A conflicting target advance produces the same stale-source refusal and leaves no MERGE_HEAD; delivery never attempts reconciliation.
3. A has close-gating children AA and AB forked from work/A. AA closes first. AB then refuses even when Git could merge cleanly. After AB incorporates current work/A, AB closes; A accumulates AA+AB and later closes normally.
4. Target movement during the gate still fails CAS. Retry refuses until the new target tip is incorporated.
5. Existing settled, empty, forge, crash-retry, and nested-delivery guarantees remain green.

## Deliverable

Amend docs/architecture.md, docs/design/bl-7b71-nested-delivery.md, skill/close.md, README/release notes, delivery code, and tests. Remove the automatic reintegration path, add the ancestry precondition, replace fold-success/conflict expectations, and add the recursive AA/AB story. Run the complete gate.