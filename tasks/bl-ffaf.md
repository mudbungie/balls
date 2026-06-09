+++
title = "Delete dep-tree: its --json is a byte-level duplicate of list --json; the human forest is the only thing it owns"
created = 1781039383
updated = 1781039383
+++
dep-tree owns no machine contract: its --json emits the same flat bedrock array as list --json (tree.rs says so itself — the nesting is derived, the consumer rebuilds). Two verbs printing one set is a drifted fact. The per-ball view (children, blockers) already lives in bl show. Burn the verb: remove from Verb::ALL/dispatch/help/skill, delete src/reads/tree.rs + tree_tests.rs, edit spec §9 (it names 'dep tree' as one of three read verbs). No replacement flag — if a human forest render is ever wanted, it is a --tree projection on list, a separate argued-up task.