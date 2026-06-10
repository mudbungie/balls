+++
title = "founding-miss stealth fallback is not durable: stealth.lock is written but never read"
created = 1781067174
updated = 1781067174
parent = "bl-72a8"
priority = 2
tags = ["bug"]
+++
bl-9857 / SS12: a rejected FOUNDING push (branch absent, no create perm) must degrade silently to stealth-local, "harmless by definition and once-per-clone".

Actual: prime does degrade silently and writes stealth.lock into the clone bundle - but nothing ever reads it. tracker/prime.rs writes the lock; effective_remote checks only the --stealth flag; zero consumers exist. So the very next `bl create` re-resolves origin, re-attempts the rejected founding push, and ABORTS loudly with the seal rolled back. The checkout is unusable for mutations: the opposite of harmless, and the silent/loud split bl-9857 demanded collapses on the second op.

Fix either way: consume the lock (durable stealth until removed), or give the per-op founding push the same silent degrade while the remote branch remains absent.