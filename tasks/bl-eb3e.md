+++
title = "bl-3616: sha-vs-ref audit of store-seal pinning + Q6 reframed as plugin-config injection (occupancy eagerness, identity shims)"
created = 1789700900
updated = 1789700900
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["design", "docs"]
+++
Maintainer 2026-09-17: (1) eager-occupancy default is configurable — users check out aggressively but not always; identity shims may express agent/user relationship; it is an injection point for plugin configuration, not a balls default. (2) Do the audit: be clear about what should pin a hash vs a ref, since the bl-22c5 reconcile rebases store seals and a rebased seal keeps content but changes sha. Deliverable: amend docs/design/bl-3616-store-tiers.md with an audit table (site, what it pins today, sha or ref or content, verdict under rebase) and the Q6 reframe.