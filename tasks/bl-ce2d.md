+++
title = "Same-ball E5 leaks git's rebase hint dump: the conflict sentence appends the raw 'git rebase FETCH_HEAD' error (7 hint: lines), and repeats the store path three times"
created = 1789711749
updated = 1789711750
claimant = "Waitresses"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["tracker"]

[[blockers]]
id = "bl-b310"
on = "close"
+++
Seen in a live two-box simulation right after bl-21ab landed: bob's stale claim of a ball alice claimed prints the (correct) named refusal, then 'Rebasing (1/1)error: could not apply … hint: Resolve all conflicts manually … Could not apply 5e9957d'. bl-3129's rule: the two contention refusals speak in balls' voice, raw git survives only for non-contention failures. The conflict IS positively identified (the unmerged index named the ball), so the git text carries nothing. Drop the ({e}) suffix from conflict(), and name the store checkout once instead of prefixing three git commands with -C <long path>.