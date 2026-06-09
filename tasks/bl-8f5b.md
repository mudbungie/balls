+++
title = "gh-plugin: harden shellback bl-create against arg-injection from GitHub titles"
created = 1780996134
updated = 1781033637
claimant = "Quill"
parent = "bl-10a4"
priority = 2
tags = ["gh-plugin"]

[[blockers]]
id = "bl-d31f"
on = "claim"
+++
shellback shells 'bl create <title>' with a GitHub-sourced (untrusted) title and no end-of-options guard; a title starting with '-' (e.g. --as) parses as a flag. Insert a '--' separator before the title positional; audit add_tag/close id args too. Mirrors core commit 938e75a0 (arg-injection guard).