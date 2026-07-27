+++
title = "A create-side plugin resolves its target repo from cwd, not the -C-addressed store: bl -C lernie create fired a workflow-dispatch attempt at mudbungie/balls"
created = 1785129746
updated = 1785130199
priority = -1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
Observed 2026-07-26: running bl-4d4b from cwd /home/mark/dev/balls printed 'could not create workflow dispatch event: HTTP 422: Workflow does not have workflow_dispatch trigger (api.github.com/repos/mudbungie/balls/actions/workflows/260389013/dispatches)'. The create sealed fine in the LERNIE store (the -C ladder held for the op itself), but some create-phase plugin appeared to resolve an origin/repo from the invocation cwd (balls) rather than from the addressed store's repo (lernie).

## Diagnosis 2026-07-27 (gudgeon) — NOT a balls defect; premise falsified

No plugin fired that line. Root cause is a bare `gh workflow run release-plz`
run from cwd /home/mark/dev/balls, whose output interleaved with the `bl -C`
create. Reproduced byte-for-byte:

    $ gh workflow run release-plz
    could not create workflow dispatch event: HTTP 422: Workflow does not have
    'workflow_dispatch' trigger
    (https://api.github.com/repos/mudbungie/balls/actions/workflows/260389013/dispatches)

Workflow 260389013 is mudbungie/balls `.github/workflows/release-plz.yml`. `gh`
resolves the repo from cwd's git origin — correct `gh` behaviour, nothing to do
with bl. The 422 itself is a REAL standing condition worth its own ball: local
`main` carries the `workflow_dispatch:` trigger (added 904ff28d, bl-7954,
2026-07-21) but **origin/main does not** — origin/main is still at 92926431
(v0.5.8, 2026-07-07). `bl close` deliberately does not push code main
(bl-e3d6), so the GitHub default branch has no dispatch trigger to fire.

Evidence the balls side is clean:

- **No installed plugin does a workflow dispatch.** The bound set is bl-chore,
  bl-delivery, bl-pushmain, bl-tracker, bl-workhours (+ unbound github-issues /
  balls-plugin-github). Not one binary contains a workflow/dispatch string, and
  nothing under ~/dev, ~/ops, ~/userconf, ~/.local/bin invokes `gh workflow run`.
- **The lernie landing has no create hook at all** (`bl -C ~/dev/lernie conf`:
  claim/close/drop/prime/unclaim × bl-delivery only). `create.post = bl-tracker`
  belongs to the BALLS landing and was never consulted — `-C` resolves the
  landing correctly, as `bl -C … conf` demonstrates.
- **The line is in neither store's op log.** Core envelopes every plugin's
  stderr into `clones/<key>/log`; a grep across every clone log on the box finds
  nothing. It never went through the plugin chain.
- **Core already hands plugins the -C-addressed path, not the raw cwd.**
  `src/dispatch.rs` replaces `edge.invocation_path` with the resolved `-C` path
  for the whole op; `src/wire.rs` `Binding::invocation_path` carries it; and
  `src/plugin.rs::spawn` pins each plugin's cwd to the addressed store's change
  worktree (`current_dir(dir)`), never inheriting the invocation cwd. This is
  already pinned end-to-end by
  `tests/invocation_scope/directory.rs::a_claim_through_the_override_hangs_the_worktree_off_the_named_project`.

Only residual core-side observation (not the reported symptom, no defect shown):
`src/clock.rs::probe` spawns the `clock_provider` binary with no `current_dir`,
so it alone inherits the raw process cwd. Its stdout/stderr are both piped and
its output is a unix-seconds integer, so it cannot leak text to the terminal.

Disposition: no balls-side change warranted for the reported symptom. Either
close as not-a-defect, or re-point this ball at the real finding — origin/main
lagging local main by a release cycle, which is why a dispatch 422s.