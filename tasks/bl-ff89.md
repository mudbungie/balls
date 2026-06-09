+++
title = "Dead-code & subtraction audit"
created = 1780970011
updated = 1780970574
claimant = "Smothering"
parent = "bl-72a8"
priority = 1
tags = ["review"]
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls/bl-ff89"
+++
Hunt removable mechanism per the subtraction principle. Find: unused fns/modules (dead_code lint, cargo-machete for deps), CLI flags / config knobs that a default or finer override already covers (severability test: removing a default should delete config, not edit code), and vestigial paths from retired designs (review verb, doctor, symlink registry, trail/next::). Confirm `git grep -E 'terminus|operating'` over src is empty.

Deliverable: list of safe deletions + the cuts applied.