+++
title = "Implement §11/§15 (bl-0af4): delete delivery-worktree field + claim.pre/unclaim.pre wirings; print path on prime.post; add 'show' read-op dispatch; fix stale bl skill --json line; close ghost bl-b930"
created = 1781030604
updated = 1781031784
claimant = "Lantern"
parent = "bl-72a8"
priority = 2
tags = ["delivery"]
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls/bl-9ccb"
+++
Spec amended in bl-0af4. Implement:
- delivery plugin: delete the delivery-worktree stage/clear (revert the field half of bl-ae51); keep recompute-from-(binding,id,claimant). Print the path on prime.post (extend the existing claimed-id loop). Add a 'show' read-op that prints the worktree for the named ball.
- core: on 'bl show' HUMAN render, dispatch the delivery read-op and fold its line in; --json stays the lossless store mirror (no dispatch). Drop claim.pre/unclaim.pre from the default hooks table.
- docs: fix the stale 'bl skill' line claiming 'bl show --json omits delivery-worktree' (it never will now — the key is gone).
- close bl-b930 (ghost: its premise — claim doesn't print, --json omits — is moot once the field is deleted and claim.post is the print).