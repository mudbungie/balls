+++
title = "chore_cli ETXTBSY flake: Cli::run spawns without retry_busy — fake-bl write-then-exec races parallel tests, aborting unrelated closes"
created = 1782969408
updated = 1782969415
claimant = "Icecap"
priority = 1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["core"]
+++
chore_cli_tests write a fake bl script then exec it via Command::output directly; a parallel test's fork inherits the open write-fd and exec gets ETXTBSY (Os code 26, ExecutableFileBusy) — reproduced ~1-in-5 running the three chore_cli tests alone, and it aborted a real bl close pre-commit gate (bl-5b09's, 2026-07-01). Core's three spawn sites already wrap exec in the bounded retry_busy (src/plugin_io.rs, per the prior ETXTBSY fix); src/chore_cli.rs Cli::run is the one spawn site that lacks it. Fix: wrap the Command::output call in retry_busy — one line, same pattern, production-harmless (bl is not a just-written file in production, the retry only ever fires in tests).