+++
title = "bl-1266 H1 fill: nested-op non-publication becomes store-scoped — core exports the held store path into plugin spawns, tracker suppresses push only on match (fail open when unset)"
created = 1789702123
updated = 1789705434
claimant = "Waitresses"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["tracker"]

[[blockers]]
id = "bl-6ed0"
on = "close"
+++
Design: docs/design/bl-3616-store-tiers.md (CONVERGED 2026-09-17). §6 Q7; fill written in docs/design/bl-1266-nested-op-publication.md §3.1 ('core exports the store path it holds into every plugin spawn (inherited through the shelling plugin, exactly as depth is), and the tracker suppresses iff binding.store matches it — failing OPEN when unset'). Maintainer 2026-09-17 ratified paying the one env: 'Variables are a smell, but not banned. They do exist for reasons.' Rule: an op does not publish an anvil an enclosing op holds open — store-scoped; depth was a proxy that coincided while every nested op was in-store. Depth stays as the recursion cap only. Update bl-1266's record (H1 → FILLED) and architecture §6/§12 env list. Prerequisite for bl-upstream.