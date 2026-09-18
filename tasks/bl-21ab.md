+++
title = "bl-tracker reconcile: fetch + rebase local seals IN the store checkout + push; once-on-reject from *.post, over N seals from sync; transport fails open"
created = 1789702121
updated = 1789702121
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["tracker"]
+++
Design: docs/design/bl-3616-store-tiers.md (CONVERGED 2026-09-17). §2.1/§3. One reconcile in bl-tracker, two callers. Per-op *.post: push; on non-ff reject, fetch + 'git rebase <remote>/<branch>' run in the store checkout (§8 bl-057a: the checkout IS the branch; never update-ref plumbing behind it) + push once more; a rebase conflict = same ball changed both sides → abort rebase, un-seal, E5 with a sentence naming the ball. sync (and prime.post): the same reconcile over every unpublished seal; conflict aborts the rebase, leaves all local seals local, names the ball. Transport failure (unreachable remote) fails OPEN in both callers: warn on stderr, store stays ahead. Only same-ball conflict or a permissions reject remains fail-closed. Pinning rule (§6.1): state it in src/seen.rs's header and §7's wire description — an unpublished seal's commit sha is scratch, never persisted. Concurrent local sealers lose their ff CAS and converge on retry — no new guard.