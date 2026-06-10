+++
title = "stealth.lock is write-only — a post-stealth mutate op rediscovers origin and founds balls/tasks anyway (§12 'locks the store local' only binds prime)"
created = 1781040083
updated = 1781061621
claimant = "mark"
parent = "bl-72a8"
priority = 2
tags = ["design"]
+++
Found while implementing bl-540e (11998f59). bl prime --stealth writes the stealth.lock self-lock and prime itself never founds/pushes — but nothing reads the lock back. A subsequent mutate op (e.g. bl create) builds a non-stealth binding, effective_remote rediscovers origin, and its */post push implicitly creates balls/tasks on origin — defeating the consent opt-out the flag exists for. The full fix (lock gates the implicit-origin tier for ALL ops, per §12:950 'locks the store local') collides with §12:957's founding-miss 're-running prime re-attempts' promise: same lock file, opposite persistence intent. Needs design dialogue before code: candidate reframes — (a) lock gates only the IMPLICIT origin tier, an explicit --remote on a later prime clears it (consent given supersedes consent withheld); (b) split 'never asked' from 'declined' into distinct lock states. Don't implement without maintainer input.