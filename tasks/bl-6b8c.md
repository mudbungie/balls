+++
title = "design: front-door edge flags require a LIVE target (refuse nonexistent and dead ids)"
created = 1781077560
updated = 1781078835
claimant = "Pencilling"
priority = 2
tags = ["design"]
+++
Design change to SS10 (frozen doc: this ball + a SS15 revision entry, never a silent edit).

Decision (converged 2026-06-10): `--needs ID[:OP]` and `--blocks ID:OP` / `--blocks OP` (create AND update alike) REFUSE a target id that is not currently live, naming the id and whether it is dead or unknown. Read-side stays unaudited: an existing edge pointing at a void remains cheap and inert (resolved = file gone) - we validate the WRITE, never audit the store.

Why:
- Under the fixed random 4-hex scheme a never-existed target can never be intentional - there is no forward-declaring an id you have not minted. It is always a typo or a hallucination, and what it produces is the models known worst failure shape: a SILENTLY ungated task (the bl-788e silent-forget class, inverted).
- A dead target is an edge born resolved - a blocker that can never block, and SS10s own bar is "a primitive must mean what it says: a blocker that does not block is not a feature." Refusal also carries exactly the information the author lacked ("that id already closed"); the remedy is dropping the flag.
- Escape hatch stays open: anyone who wants to stitch the dead does it by hand - `bl import` remains verbatim (a reproduction, not a transition - SS9 "no enforce gate" unchanged), and a direct store edit is always possible. We take no risk to help that case through the front door.

Unaffected: the SS16 epic edge-mint (targets live children only); existing dangling edges in stores (read-side inert, `--no-needs` unlinks).

Deliverable: SS10 edit + SS15 log entry in docs/architecture.md, then the refusal in create/update flag handling (one live-store existence read at the flag seam).