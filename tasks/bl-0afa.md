+++
title = "A create-side plugin resolves its target repo from cwd, not the -C-addressed store: bl -C lernie create fired a workflow-dispatch attempt at mudbungie/balls"
created = 1785129746
updated = 1785129746
priority = -1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
Observed 2026-07-26: running bl-4d4b from cwd /home/mark/dev/balls printed 'could not create workflow dispatch event: HTTP 422: Workflow does not have workflow_dispatch trigger (api.github.com/repos/mudbungie/balls/actions/workflows/260389013/dispatches)'. The create sealed fine in the LERNIE store (the -C ladder held for the op itself), but some create-phase plugin resolved an origin/repo from the invocation cwd (balls) rather than from the addressed store's repo (lernie) — cross-repo confusion in exactly the direction -C exists to prevent. Identify which installed plugin attempts a workflow dispatch on create (check the balls landing's plugins.toml chains and the lernie landing's), and make plugin repo-resolution ride the same -C-resolved project path the core op uses (the wire carries it; nothing should re-derive from cwd). Also decide whether a plugin's remote failure should print raw HTTP on an otherwise-successful create — per the fail-open ladder it should be a one-line warn at most.