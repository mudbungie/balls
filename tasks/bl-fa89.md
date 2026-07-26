+++
title = "Store-seal contention speaks git's voice, not balls': 'Not possible to fast-forward' should say the store moved, re-run"
created = 1785027733
updated = 1785027733
parent = "bl-ea55"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
MODE 3 of bl-cdec ('create/close under concurrent store writers still occasionally needs the documented single retry'). The store seal's ff-only IS the compare-and-swap and it behaves correctly — nothing is overwritten, the retry converges. But the operator sees raw git ('fatal: Not possible to fast-forward, aborting' or 'cannot lock ref HEAD'), which reads as corruption rather than as the one-line instruction it is.

bl-a3bb already set the precedent for the delivery ref: a rejected CAS gets balls' voice naming what happened and what to do. Do the same at src/git.rs's ff-only: 'the store moved under this op (a concurrent bl won the seal) — nothing was written; re-run'. Detection is the existing Err path; this is wording plus a test, no new mechanism.

Deliberately NOT in scope: an automatic bounded retry in core. Converge-on-retry (§14) is the documented rule and the retry is one command; an in-core loop hides contention and doubles wall-clock on a genuine conflict. Revisit only if the clean voice proves insufficient under load.