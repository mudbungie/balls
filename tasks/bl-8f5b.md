+++
title = "gh-plugin: harden shellback bl-create against arg-injection from GitHub titles"
created = 1780996134
updated = 1780996492
claimant = "Scab"
parent = "bl-10a4"
priority = 2
tags = ["gh-plugin"]
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls-github-plugin/bl-8f5b"
+++
shellback shells 'bl create <title>' with a GitHub-sourced (untrusted) title and no end-of-options guard; a title starting with '-' (e.g. --as) parses as a flag. Insert a '--' separator before the title positional; audit add_tag/close id args too. Mirrors core commit 938e75a0 (arg-injection guard).