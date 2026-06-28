# Legacy → greenfield migration runbook (§16)

The one-time cutover of a repo from pre-greenfield balls (task JSON under
`.balls/tasks/` on the old `balls/tasks` branch) to the greenfield store. The
data step is verbs, not a script — see architecture.md §16 and
docs/design/bl-e614-import.md. Each step is ordinary; only the sequence and
the two guards are migration-specific, which is why this is a runbook and not
code.

1. **Quiesce.** Finish or release every claimed legacy task (merge/close/
   unclaim in-flight work). `import` carries `claimant` verbatim, but a
   claimed task's in-flight code worktree is NOT reproduced — worktrees
   materialize at `claim` only (bl-c2bf), so an imported-claimed task has none
   until you `unclaim` + re-`claim`, which won't restore the original's lost
   uncommitted work. This is the operator's guard, deliberately not enforced in
   code: `import` is a general primitive (federation, restore) and must not
   carry one caller's policy.

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

5. **Join the histories and cut over.** The greenfield store REUSES the
   `balls/tasks` name, and bl NEVER rewrites the legacy ref (the §12
   quarantine, bl-868d) — and neither does the cutover: the one explicit,
   human-coordinated act is a history JOIN that makes the greenfield tip a
   descendant of the legacy tip, so the cutover push is an ordinary
   fast-forward, not a rewrite. From the XDG store checkout
   (`$XDG_STATE_HOME/balls/clones/<pct-enc-path>/tasks`):

   ```
   git fetch <origin-url> refs/heads/balls/tasks
   git merge -s ours --allow-unrelated-histories FETCH_HEAD \
     -m "cutover: greenfield store supersedes the legacy .balls/ JSON (§16)"
   git push <origin-url> balls/tasks:refs/heads/balls/tasks
   ```

   `-s ours` keeps the greenfield tree byte-for-byte (no `.balls/` resurrected)
   while parenting the merge on the legacy tip. No force, no lease, no archive
   branch: every clone's `origin/balls/tasks` fast-forwards on its next fetch,
   and the legacy history stays where it always lived — the early history of
   `balls/tasks`, with every closed legacy task still readable at the merge's
   legacy parent. The final plain push is just the ordinary publish brought
   forward — once the join exists, the next op's sync/publish would deliver
   the cutover the same way; then federation resumes as on any checkout.

6. **Per-plugin adoption.** Each plugin re-adopts its own legacy territory
   (§16) — e.g. github-issues' one-time `adopt` stamps the `[bl-id]` title
   markers so the first `sync` joins without duplicates.

Post-cutover, retire the shim: the `--legacy` flag and `src/reads/legacy.rs`
are severable — deleting them deletes code, not core (§16).
