+++
title = "PRIME.md: prime dumps the landing's repo brief (agent context bootstrap)"
created = 1785997125
updated = 1785997125
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
A fresh agent primes and learns nothing about THIS repo. `bl prime --skill`: "It prints no listing of its own." The repo's hard-won rules exist and are correct, but balls never hands them over — today AGENTS.md fills the hole only because the harness happens to load it. Point balls at a repo whose harness does not, and the tracker says nothing.

FIX: if `config/PRIME.md` exists on the landing, `prime` prints it verbatim to stdout. That is the whole mechanism.

WHY IN CONFIG, NOT THE REPO: install is a pure path-copy (arch §6, folder = mirror), so a brief on the landing rides `bl prime --center <hub>` for free — enroll a checkout and it adopts the project brief with its config. Fleet-wide, one authority. Inert markdown is the safest possible config payload (all config is potential RCE; this carries none).

DECISIONS, each a subtraction:
- NO seed. default-config/ ships no PRIME.md. Absence is silence; a seeded template would print boilerplate on every prime forever.
- NO flag. Print unconditionally, every prime. Any --quiet is the smell. "Too long to print each time" is the file's fault, fixed in the file.
- NO new verb, field, or store.

THE DISCIPLINE THAT KEEPS IT FROM ROTTING: PRIME.md is a POINTER, not a knowledge base. "Read docs/architecture.md §9 before touching close" — never a restatement of §9. This is the whole argument against beads' `bd remember`: an insight stored next to the code it describes is corrected by the diff that invalidates it; a restated fact in a sidecar store is never in anyone's diff, and rots silently. Rot is worse than absence, because agents trust it. Pointing cannot drift, and it keeps PRIME.md from becoming a second home for AGENTS.md.

Context: opening rung of the beads-comparison memory evaluation. Compaction is explicitly NOT adopted (the live tree IS the compaction); historical annotations deliberately out of scope.