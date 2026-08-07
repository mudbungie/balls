# AGENTS.md

This repo uses **balls** (`bl`) for task tracking — and builds it. Run `bl --skill` for the
operating guide, then `bl <cmd> --skill` before you run any command; each verb documents its own
flags and semantics there.

Session start is `bl prime --as YOUR_IDENTITY`, then `bl list`. The identity comes from your
harness, never from you — see *Identity* in `README.md` for why.

`docs/architecture.md` (§0–§16) is the frozen design reference and the authority for behavior. Where
it and the code disagree, one of them is wrong: fix that, do not route around it. Never implement a
deviation silently — amend the doc.

`README.md` covers install, the pre-commit gate (`make hooks`), and the repo bootstrap this file is
part of.

Pointers, not copies — this file names where things live and restates none of them.
