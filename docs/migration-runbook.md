# Legacy → greenfield migration runbook (§16)

The one-time cutover of a repo from pre-greenfield balls (task JSON under
`.balls/tasks/` on the old `balls/tasks` branch) to the greenfield store. The
data step is verbs, not a script — see architecture.md §16 and
docs/design/bl-e614-import.md. Each step is ordinary; only the sequence and
the two guards are migration-specific, which is why this is a runbook and not
code.

1. **Quiesce.** Finish or release every claimed legacy task (merge/close/
   unclaim in-flight work). `import` carries `claimant` verbatim, but a
   claimed task's in-flight code worktree is NOT reproduced — `bl prime` would
   re-materialize a fresh `work/<id>`, stranding the old one. This is the
   operator's guard, deliberately not enforced in code: `import` is a general
   primitive (federation, restore) and must not carry one caller's policy.

2. **Prime.** `bl prime` founds the greenfield substrate — the `balls/config`
   landing (the seed IS the migrated config; the legacy knob pile dissolves,
   §16) and an empty store. There is no config-rewriting step. A shared
   `origin` still carrying the LEGACY `balls/tasks` is fine (bl-868d): its tip
   has no `tasks/`, so prime QUARANTINES it — warns, adopts nothing, founds
   the fresh store — and until step 5 every op's sync/publish warns and keeps
   work local instead of failing against the un-cut-over ref.

3. **Preview.** `bl list --legacy` (add `=REF` if the legacy store is not at
   `balls/tasks:.balls/tasks` — in a fresh clone the legacy history exists
   only remotely, so use `--legacy=origin/balls/tasks`) — the migration
   dry-run. What it lists is exactly what migrates: live tasks only, the §16
   field map applied, notes folded into bodies. `bl show <id> --legacy`
   inspects any one projection.

4. **Migrate.** `bl import --legacy` (same `=REF` rule as step 3) — imports
   every live task verbatim (ids and timestamps preserved) and wires the epic
   reciprocal edges (each live child claim-blocks its live parent) through
   ordinary update ops. One command; on any collision it refuses the whole
   stream naming the ids (nothing half-lands — fix and re-run). The
   composable form is the same thing: `bl list --legacy --json | bl import`.

5. **Cut the shared ref over.** The greenfield store REUSES the `balls/tasks`
   name, and bl NEVER rewrites the legacy ref implicitly (the §12 quarantine,
   bl-868d) — the cutover is this one explicit, human-coordinated force-push.
   Keep the legacy history first if wanted:
   `git branch balls-archive origin/balls/tasks`. Then, from the XDG store
   checkout (`$XDG_STATE_HOME/balls/clones/<pct-enc-path>/tasks`):
   `git push --force-with-lease=refs/heads/balls/tasks:<legacy-sha> <origin-url>
   balls/tasks:refs/heads/balls/tasks` (the explicit lease form — the store
   checkout has no remote-tracking ref to lease against implicitly). The next
   op's sync/publish then resumes as on any federated checkout.

6. **Per-plugin adoption.** Each plugin re-adopts its own legacy territory
   (§16) — e.g. github-issues' one-time `adopt` stamps the `[bl-id]` title
   markers so the first `sync` joins without duplicates.

Post-cutover, retire the shim: the `--legacy` flag and `src/reads/legacy.rs`
are severable — deleting them deletes code, not core (§16).
