+++
title = "README: a bootstrap section — how to configure a repo so agents find balls (AGENTS.md points, PRIME.md briefs, the harness names)"
created = 1786064575
updated = 1786064592
claimant = "Pale"
priority = 2
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"

[[blockers]]
id = "bl-6549"
on = "close"
+++
Cold start is the real integration gap. balls ships good agent docs (SKILL.md via `bl --skill`, per-verb `bl <cmd> --skill`) and a landing brief (`config/PRIME.md`, bl-c84f), but the README never says how an operator wires a repo so a fresh agent finds any of it. Today the setup is scattered: the brief is documented under prime, identity is a prose paragraph ending in a `shuf` recipe, and the AGENTS.md hop — the only thing that tells an agent balls exists at all — is nowhere.

Raised while evaluating beads integration surface (`bd setup {claude,codex,cursor,factory,mux}`, an MCP server on PyPI, AGENTS.md generation). Almost all of it is correctly refused: the vendor matrix fails the §0 severability test (removing a default must delete config, not code), and beads own MCP README concedes CLI+hooks beats MCP wherever a shell exists (~1-2k tokens vs 10-50k of schema). AGENTS.md GENERATION is structurally impossible in core besides — §11, base balls never opens the project repo. What survives is documentation, not mechanism: one section stating the three-piece bootstrap and the division of labor between the pieces.

That division is the content worth writing down, and it is what stops PRIME.md becoming a second home for AGENTS.md — already asserted at architecture.md §15 (bl-c84f) but never stated where an operator reads it. AGENTS.md lives in the PROJECT tree, is loaded by the harness, and does DISCOVERY (balls exists; run `bl --skill`) — the operator writes it, balls cannot. PRIME.md lives on the LANDING, is printed by every prime, and carries THIS PROJECT S rules — it travels to the whole fleet through `install`, which AGENTS.md does not. The harness supplies identity via `--as`, because balls cannot see session boundaries and a model naming itself collapses to three Junipers.

Pointers, not copies, applies to both files.

Deliverable: a README section. No verb, no flag, no code.