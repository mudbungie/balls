+++
title = "Missing-task-file reads must refuse in balls voice, not raw errno"
created = 1784525701
updated = 1784525779
claimant = "Revises"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]

[[blockers]]
id = "bl-7208"
on = "claim"
+++
CONFIRMED by tests/guards.rs (bl-7208) + tests/crash_recovery.rs (bl-d826): bl claim <closed-id> and bl close <missing-id> exit with bare "bl: No such file or directory (os error 2)". Occupancy::stage and Retire::stage (src/change.rs) open with read_task(dir,&id)? and taskfile::read_task (src/taskfile.rs:28-31) is fs::read_to_string(task_path)?, so the io::Error surfaces verbatim. Absence IS the record — but the refusal must speak: map NotFound at the call sites (or in read_task) to "no such ball: <id> (closed tasks have no file — absence is the record)" or similar in-voice wording; distinguish nothing (closed vs never-existed is undecidable there by design — one message for both is correct). Keep the untouched-store guarantee. FLIP the pinning assertions in tests/guards.rs (claim_on_a_closed_ball_..._leak_the_raw_read_errno) and the FINDING pin in tests/crash_recovery.rs to the new message. 100% coverage: the new arm needs a src-side unit test (tarpaulin ignores tests/). Doc touch only if a skill doc quotes the old error.