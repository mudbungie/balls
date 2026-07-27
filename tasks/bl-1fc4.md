+++
title = "bl create id-collision abort (create.pre reassignment) + parent field accepts non-id garbage"
created = 1785028335
updated = 1785124166
claimant = "fathom"
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-4b5b"
on = "close"
+++
Observed on lernie 2026-07-25 (agent Willow): a bl create aborted with 'a create.pre plugin reassigned the new task to bl-1783, which already exists — id collision', and the caller's follow-up gate-creation loop then created three stray balls whose parent field held the literal string '--needs' (bl-9e25/bl-588c/bl-4a9e on lernie; closed as discarded). Two issues: (1) the create.pre id-reassignment can collide with an existing id and aborts instead of re-rolling; (2) after the abort, argument parsing of the subsequent create treated flag tokens as values (or the loop mis-quoted — but the store accepted '--needs' as a parent id, so validation should reject a parent that is not a plausible task id).