+++
title = "bl-chore should not nest at all: fold the mint into claim.pre as file writes, deleting the nested op, the scratch record and the rollback"
created = 1786597914
updated = 1786597915
claimant = "Enthused"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
The fold §4 of docs/design/bl-1266-nested-op-publication.md argues for, now that
the outermost-publishes rule (main 9f3d1bf4) has closed the class.

§14's appendix exists for effects whose binding artifact lives in an EXTERNAL
system — the jira ticket core cannot reach into to make part of the atom.
bl-ffbf extended it to a nested `bl` op, calling balls "the external tracker
that assigns its own id". That is the error: balls is not external to itself.
Core CAN reach the store — that is the change worktree, and `pre` is the
sanctioned door (§8 step 2: "pre modifiers ... edit the shared worktree (rename
the ball file to reassign an id, edit frontmatter)"; §8.3: "a `pre` plugin edits
the SHARED change worktree, so it can also touch a SIBLING tasks/*.md").

So bl-chore's mint belongs in claim.pre, writing files:
  - write tasks/<child>.md per configured chore into the change worktree;
  - add the {id, on: close} blocker to the parent's file, already in that
    worktree since claim is staging its claimant there.
One seal, one commit, one push — the mint becomes part of the claim's atom
instead of an artifact keyed to an op that may never seal.

WHAT IT DELETES, and this is the point — subtraction, not relocation:
the nested `bl create`; the `Bl` shell seam and src/chore_cli.rs; the whole of
src/chore_scratch.rs (the ids never cross a process boundary, so there is
nothing to carry); bl-chore's rollback and its mid-list inline unwind; the
close.post record sweep (bl-f88b) — nothing to sweep; and §14's nested-op
paragraph. The rollback disappears because the mint is no longer a separate
effect to undo: an aborted claim discards the change worktree, taking the
children with it.

epic-skip's `bl list --json` is the other nested op and goes the same way: the
children are readable from the change worktree the plugin stands in, so the
predicate becomes a directory scan and the Bl seam has no remaining caller.

SETTLED IN THE DESIGN, do not relitigate: (a) a `pre` plugin minting a NEW id
is not a gate to open — nothing enforces "core mints, plugins reassign", the id
scheme is public and fixed, and the live set to re-roll against is in the
worktree; what is owed is a doctrine sentence in §8, not a mechanism. (b) The
§5 one-act-per-commit objection dissolved: a child born in a commit whose
subject is `claim <parent>` reads its creation as "claim", which is ACCURATE —
the child exists because the parent was claimed. No §5 change.

Wiring moves with it: claim.post -> claim.pre for bl-chore, and close.post
drops entirely.