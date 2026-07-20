+++
title = "Where am I? bl from a subdirectory and from inside a work worktree"
created = 1784525386
updated = 1784525386
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bl-covtest"]
+++
PROBE. src/main.rs sets invocation_path to the LITERAL env::current_dir() — no git-root discovery anywhere in src/ — and clone_dir() percent-encodes it to key the whole XDG clone bundle. By construction, bl from myproject/src/ vs myproject/ may be TWO different stores for the same project, silently. (1) cd into a subdir of a primed project: does bl list see the real store or silently found an invisible sibling? (2) cd into a claimed work/<id> worktree and run bl list/claim: same question. Whatever the behavior IS, pin it in a test and report it faithfully — if invocation is root-relative only by accident, or splits silently, that is an architecture finding to RAISE, not to quietly fix. New file tests/invocation_scope.rs.