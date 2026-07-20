+++
title = "bl conf changes take functional effect, not just file effect"
created = 1784525385
updated = 1784525398
claimant = "Revises"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]
+++
conf writes are only ever asserted as file/provenance state — never as behavior: (1) bl conf set task-remote <bare-B>: subsequent create/close traffic lands on B not A, and reads back correctly. (2) Unwiring: a marker-stamping plugin fires, conf remove <op>.<phase> <name>, the same op no longer stamps (the schedule change actually stops dispatch). (3) The legacy XDG global remote tier used OPERATIONALLY: seed ~/.config/balls/config.toml remote with no binding/sentinel/origin; prime/create actually founds and pushes through it (the pre-per-clone-binding machine upgrade story). New file tests/conf_functional.rs.