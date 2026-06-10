+++
title = "bl install unusable on a standard federated checkout: install.pre fetch hard-aborts when the remote lacks balls/config"
created = 1781067145
updated = 1781067145
parent = "bl-72a8"
priority = 1
tags = ["bug"]
+++
Default schedule wires install.pre = [tracker], which fetches balls/config from the resolved remote. A remote without that ref is fatal: `tracker: git fetch <hub> balls/config: fatal: couldnt find remote ref balls/config` -> plugin abort -> the whole, purely local, install fails.

But the landing is NEVER pushed by design (SS4 single-owner; verification confirmed no bl op ever publishes balls/config), so a stock hub never carries it: every `bl install` on an ordinary origin-backed checkout fails until someone hand-pushes a balls/config outside bl.

SS13 describes this fetch as "a READ only ... stealth (no remote) is a no-op". A present remote MISSING the ref must be a no-op too: only an adopt that actually names the center as --from needs the fetched config, and that case can fail at point-of-use. Local branch-to-branch installs should never depend on remote state.