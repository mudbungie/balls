+++
title = "bl import of a foreign-rooted ball succeeds silently into invisibility: hint on stderr when root_commit is foreign to the checkout"
created = 1785125449
updated = 1785129563
claimant = "caulker"
priority = -1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
Found during bl-36f1 (main 03a6e7b0). Importing a ball whose root_commit is foreign to the checkout prints 'import 1 ball' and then default bl list shows nothing — the ball is real, show resolves it, list --everywhere reveals it; the root-aware default scope is behaving exactly as designed. Correct, but silent in practice: the operator's next command appears to contradict the import's confirmation. Fix in the existing capability-hint voice ([source] hints decorate refusals — same doctrine): when any imported record's root_commit is foreign to the checkout, add one stderr line naming the fact and the lifted-scope read (list --everywhere). No new flag, no behavior change — a hint where silence currently reads as loss. Wording + a test.