+++
title = "bl close must not push the code remote — revert bl-2656"
created = 1782794444
updated = 1782794444
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["core", "cicd"]
+++
Revert bl-2656's code-main auto-push. Maintainer-converged 2026-06-29 (dialogue): auto-pushing the CODE remote on close is balls being opinionated about the user's git, which it has no business doing.

WHY (the reframe): the store push (balls/tasks) is balls' OWN bookkeeping — auto-publishing it is housekeeping. The code 'main' is the USER's, and pushing it is a social publish decision. bl-2656 leaned on a FALSE symmetry between the two. Auto-push also removed the human publish gate (where the cut-a-PR / caught-a-bug-while-still-private decision lives) and turned a reversible local act into an immediately-public one — a footgun for any human/PR-ish user. And it contradicts bl-c3c0's just-made 'messaging, not auto-action' stance. A close-time SIGNAL was also rejected: close != push is universally understood; balls should not nag about the user's workflow.

WHAT: revert d6c8a887 in full — bl close goes back to local-squash delivery + the tracker store push, nothing touching the code remote. Restores the pre-bl-2656 tree exactly (push_integration gone, delivery_wire.rs re-inlined + deleted, docs reverted). Drops bl-2656's 'release auto-prepares' too: release-plz prepares when YOU push main, a deliberate step again.