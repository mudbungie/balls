+++
title = "Atomicity as a core guarantee: one CAS commit point per repo per op — audit every op, close the gaps"
created = 1785027722
updated = 1785027722
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["design"]
+++
bl-cdec reports close failing under 15-way concurrency. The report is three symptoms of one missing invariant: balls has the components for atomicity (git objects are inert until a ref moves; update-ref takes an expected old value) but has never STATED the guarantee, so each op reinvents its own commit point and two of them are wrong.

Philosophy: git's porcelain is segmented too — write objects, build a tree, move a ref — and it is atomic anyway because the segments before the ref move are INERT and the ref move is a compare-and-swap against the value the work was derived from. Failure is a non-event: index.lock is written-then-renamed, a rejected ref update leaves dangling garbage nobody can see. Nothing is rolled back; the caller re-derives. balls already believes this (§14 converge-on-retry, the BINDING/NON-BINDING split) but never made it an INVARIANT with obligations, so the delivery CAS validates the wrong old value and the store seal's failure destroys state its own abort path reads.

Deliverable: docs/design/bl-cdec-atomicity.md (living document) — the guarantee stated as obligations, an audit of every op against it, and the gap list — plus the §0 principle + §15 entry in docs/architecture.md that RECORDS it. The implementation is filed as children.