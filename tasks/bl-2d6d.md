+++
title = "Security review (subprocess, git, paths, secrets)"
created = 1780970014
updated = 1780970584
claimant = "Liveried"
parent = "bl-72a8"
priority = 1
tags = ["review"]
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls/bl-2d6d"
+++
Security review of subprocess/plugin execution and git handling. Check: git command construction for argument/shell injection (src/git.rs, src/bin/tracker.rs, tracker/git.rs); env handling (GIT_* stripping, ETXTBSY on re-exec); path/encoding safety (percent-encoded clone paths vs mirrored worktree paths); the plugin trust boundary §6 (invocation-tree recursion cap, protocol-drift rejection at install); remote push/fetch trust; secret handling for the github-issues token/base.json contract (sibling repo, but note the seam).

Deliverable: threat findings ranked by severity + remediations.