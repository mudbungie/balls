+++
title = "Pure-note no-op seal silently drops the -m narration — fail loudly instead"
created = 1781058454
updated = 1781058454
priority = 2
+++
A zero-edit `bl update <id> -m NOTE` relies on the `updated` restamp (seconds granularity) to produce a diff. When the ball's last write landed in the same wall-clock second, the §8 seal converges on the existing tip (the no-op seal), no commit is created, and the -m narration — the entire payload of a notes-via--m update — vanishes while the verb still prints its `update <id>` confirmation.

Why fail instead of --allow-empty: the no-op seal is the §13 idempotence story and stays correct for every -m-less op; the narration is the one payload that cannot ride a no-op. Make the loss loud: when the op carried -m and the seal produced no commit, the op ERRORS (and rolls back like any seal failure) instead of confirming. A retry a second later succeeds.