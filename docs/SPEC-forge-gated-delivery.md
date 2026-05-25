# SPEC: Forge-Gated Delivery and Configurable Integration Branch

Status: draft
Scope: introduces an opt-in integration mode where `bl review` defers the squash to an external forge (GitHub PR, GitLab MR, etc.), and an opt-in configurable integration branch. The default flow (direct local squash to the current branch at the repo root) is unchanged.

This SPEC was originally written against the pre-XDG schema (`delivery.mode = local-squash | deferred`, `review.pre_check`, repo-level `target_branch` on `.balls/config.json`). SPEC-clone-layout §6.6 / §6.7 renamed those fields and removed the repo-level `target_branch`; the current names land in this document with the bl-717e cross-doc sweep. `delivery.mode` is now `integrate.mode`, with values `direct` (the old `local-squash`) and `forge-pr` (the old `deferred`); `review.pre_check` is now `review.gate_command`; repo-level `target_branch` is gone, replaced by the resolution chain `task.target_branch ?? HEAD@root`.

## 1. Motivation

Balls's default lifecycle assumes trunk-based delivery: `bl review` squashes the work branch into whatever is checked out at the repo root, immediately and locally. That fits a solo agent on a personal repo. It does not fit two real and common situations:

1. **External code review.** A repo with required reviewers and CI gates produces the merge commit *via the forge*, after approval. The local squash that `bl review` performs today either bypasses the forge entirely or has to be undone before the PR can be opened. Either way, the agent cannot finish a task by running `bl review` — the lifecycle stops short of the external gate balls knows nothing about.
2. **Git-flow / multi-branch integration.** Feature work targets `develop`; hotfixes target `main`. The repo has more than one integration branch at the same time. Today, balls reads "whichever branch is checked out at the repo root" and silently targets that. A stray `git checkout` at the repo root re-targets every subsequent review.

The two problems share a fix: make the integration target *explicit and configurable*, and add an integrate mode where `bl review` hands off to the forge instead of squashing locally. Together they let balls model the workflow a contributor would otherwise be doing by hand around `bl`, without breaking the existing default.

## 2. Principles

Invariants. An implementation that violates one is out of scope; revisit the spec instead.

1. **Default is unchanged.** A repo with no new config behaves bit-identically to today. Existing tests are a regression suite.
2. **Opt-in via config, not via convention.** Modes and target branches are read from `.balls/config.json` (or per-task on the task file). No flag-on-the-CLI mode-switching; modes are repo-level facts.
3. **No new `Status` variant.** `review` is reused. The semantic difference between "squashed locally, one command from done" and "branch pushed, awaiting forge merge" is expressed via existing primitives (`gates` link + state of the gate child task), not via a new lifecycle node.
4. **`gates` enforces the external hold.** The "awaiting external merge" state is a parent task in `review` with an open `gates` link to a child task; the existing close-blocker on `gates` is what makes `forge-pr` mode backwards-compatible with old `bl`.
5. **Delivery tag is ground truth.** `[bl-xxxx]` in the integration-branch commit subject remains the source of truth for what shipped. `delivered_in` is a hint, populated opportunistically. This is unchanged from today; `forge-pr` mode relies on it.
6. **Plugin contract extends, does not replace.** The forge plugin uses the same protocol as Jira/Linear/Issues plugins. Core remains forge-agnostic.

## 3. Terminology

- **integration branch**: the branch into which a task's work is delivered. Today: whatever HEAD points to at the repo root. After this spec: the resolved target (per-task `target_branch` → `HEAD@root`).
- **integrate mode**: one of `direct` (default, today's behavior — the old `local-squash`) or `forge-pr` (the old `deferred`). Per-clone setting on `repo.json` (post-XDG); a project-wide default may live on `project.json`, a per-on-disk-checkout override on `clone.json`.
- **gate child**: a task auto-created by `bl review` in `forge-pr` mode, linked from the parent via `gates`. Holds the parent's `bl close` until the forge merges the PR.
- **forge plugin**: a plugin (per README §Plugin System) that knows how to open PRs, poll PR state, and close gate children when a PR merges. Distinct from an *issue-tracker* plugin (Jira, Linear, GitHub Issues).

The word "PR" in this document means "the forge's pre-merge code review unit": GitHub Pull Request, GitLab Merge Request, Gitea PR, etc. Plugins are forge-specific; the core is not.

## 4. Topology

Two integrate modes coexist in the same `bl` binary. A repo picks one. The repo's config selects which branch of the lifecycle a `bl review` follows.

```
direct mode (default, unchanged):

  in_progress  ──bl review──▶  review  ──bl close──▶  archived
                                  │
                                  └─ work branch torn down
                                     squash commit on integration branch carries [bl-xxxx]

forge-pr mode (opt-in):

  in_progress  ──bl review──▶  review + gate child open  ──┐
                                  │                         │ forge merges PR
                                  │                         ▼
                                  │                       gate child closes
                                  ▼                         │ (manual or plugin)
                              bl close blocked              │
                                  ▲                         │
                                  └─────────────────────────┘
                                               │
                                               ▼
                                           bl close ──▶ archived
                                           [bl-xxxx] on integration branch is the
                                           forge-produced merge commit
```

The two modes converge at `archived`: in both cases there is a squash-style commit on the integration branch carrying `[bl-xxxx]`, and the task file is removed from the state branch tip.

## 5. Config schema additions

The integrate mode lives on `repo.json` (post-XDG; SPEC-clone-layout §6.3), with an optional project-wide default on `project.json` and a per-on-disk-checkout override on `clone.json`. `review.gate_command` follows the same layering. Repo-level `target_branch` was removed in SPEC-clone-layout §6.7: only per-task `target_branch` exists, with `HEAD@root` as the fallback. `min_bl_version` is tracker-scope and lives only on `project.json`.

`repo.json` snippet (every field optional; defaults shown):

```json
{
  "integrate": {
    "mode": "direct"
  },
  "review": {
    "gate_command": null
  }
}
```

`project.json` may carry the same `integrate` / `review` blocks as project-wide defaults, plus the tracker-scope `min_bl_version`.

| Field | Type | Default | Meaning |
|---|---|---|---|
| `integrate.mode` | `"direct"` \| `"forge-pr"` | `"direct"` | Selects the `bl review` code path. `direct` squashes locally; `forge-pr` defers to the forge. |
| `review.gate_command` | string \| null | null | Shell command run after merging the integration branch into the worktree and *before* the squash. Non-zero exit aborts review. `null` disables the gate. |
| `min_bl_version` | string \| null | null | Advisory. Newer `bl` clients warn if their version is below. Older clients ignore. **Owned by `project.json`** (tracker-scope per SPEC-clone-layout §6.5); rejected on `repo.json` or `clone.json`. |

An absent `integrate` block is equivalent to `{"mode": "direct"}`. The legacy field names (`delivery.mode`, `delivery.mode = "local-squash"|"deferred"`, `review.pre_check`) are accepted under the Phase 1 dual-read with an in-memory translation; a freshly-written `repo.json` rejects them outright (SPEC-clone-layout §14.16 / §14.17).

## 6. Task schema additions

One new optional field on `Task`:

| Field | Type | Default | Meaning |
|---|---|---|---|
| `target_branch` | string \| null | null | Per-task integration branch. Use case: a hotfix task targeting `main` on a repo whose default branch is `develop`. The clone-level `target_branch` was removed in SPEC-clone-layout §6.7; the resolution chain is `task.target_branch ?? HEAD@root`. |

`Task` already lacks `#[serde(deny_unknown_fields)]` (src/task.rs:114), so old `bl` decoding a task with `target_branch` set silently drops the field. This is the load-bearing piece of backwards compatibility for git-flow.

## 7. `bl review` mechanism

Resolved at review time, in order:

```
effective_target_branch = task.target_branch
                       ?? git_current_branch(&store.root)
```

A user who wants a repo-wide default checks out the target branch at the repo root before `bl claim` (SPEC-clone-layout §6.7).

### 7.1 Direct mode (default)

Unchanged from today (src/review.rs:80–180). Squashes `work/bl-xxxx` into `effective_target_branch` locally, writes `delivered_in`, flips status to `review`, leaves worktree intact. If `task.target_branch` is unset everywhere, the effective target is whatever's checked out at the repo root — byte-identical to behavior from before this spec.

### 7.2 Forge-PR mode

`bl review bl-xxxx -m "..."` in `forge-pr` mode:

1. Commit uncommitted worktree changes (`wip: bl-xxxx`), same as today.
2. Merge `effective_target_branch` into the worktree. Conflicts fail review here, same as today.
3. Push `work/bl-xxxx` to `origin`. Failure to push fails the review with the worktree intact; retry after fixing remote access.
4. Auto-create a gate child:
   - `bl create "Forge: PR merged for bl-xxxx" -t task --parent bl-xxxx --tag forge-gate`
   - `bl link add bl-xxxx gates <child_id>`
5. Flip parent status to `review` on the state branch. `delivered_in` is left null.
6. Print to stdout: a recommended PR title ending in `[bl-xxxx]`, the gate child id, and the work branch name. Stderr carries a one-line note that the parent is now gated.

`bl review` in `forge-pr` mode does **not** touch `effective_target_branch` locally and does **not** set `delivered_in`. Both happen later, when the forge merges the PR.

### 7.3 Rejecting a forge-PR review

A reviewer rejection is the same surface as direct mode: `bl update bl-xxxx status=in_progress --note "..."`. Implementation must additionally close the gate child as part of the same state-branch commit (or refuse the update if the gate child cannot be closed atomically). This keeps the invariant: a task is `in_progress` iff no open gate child exists.

The work branch on `origin` is left alone. The operator (or forge plugin) is responsible for closing the PR via the forge if they want it abandoned.

## 8. `bl close` mechanism

`bl close` is unchanged in its observable shape: it archives the task on the state branch, tears down the worktree, removes `work/bl-xxxx`. Two internal changes:

1. **Gates check (existing).** A parent task with an open `gates` link is refused close. This is the BC-load-bearing point: an old `bl` running `bl close` on a `forge-pr`-mode parent already refuses, because gates were enforced before this spec.
2. **`delivered_in` opportunistic resolution (new).** If `delivered_in` is null on the closing task, run the existing tag-scan (src/delivery.rs:48) against `effective_target_branch` and populate the hint in the close commit. If the scan finds nothing, emit a warning and close anyway — the half-push detector (src/commands/half_push.rs) catches the "state says closed, main has no tag" case as before.

A new optional flag `bl close --delivered <sha>` overrides the scan unconditionally, for the case where the forge produced multiple commits and the operator wants to point at a specific one.

## 9. Forge plugin contract

Forge plugins (e.g. `balls-plugin-github`) implement the existing plugin protocol (README §Plugin System) with these behaviors:

| Command | Behavior |
|---|---|
| `auth-setup`, `auth-check` | Standard. Forge-specific token entry and validation. |
| `push --task ID` | Iff the task is `status=review` under `forge-pr` mode with a `work/bl-xxxx` pushed to origin: open or update the forge PR against `effective_target_branch`. PR title must contain `[bl-xxxx]`. Store `{ pull_request: { number, url, head_sha, target_branch } }` in `task.external.<plugin_name>`. |
| `sync [--task ID]` | For each `forge-pr`-mode task in `review` with an open PR, poll forge state. On merged: emit a sync-report `updated` entry that closes the gate child, carrying the merge commit SHA in `add_note`. Core's existing sync-report processing closes the gate child, which unblocks the parent's close. |

The plugin does not call `bl close` directly on the parent. It closes the gate child; the operator (or another automation) closes the parent. This keeps the plugin from owning the entire lifecycle.

A `balls-plugin-github` implementation is specced as a separate ball (`bl-25aa`); other forges follow the same contract.

## 10. Backwards-compatibility audit

Walk the matrix: an old `bl` (pre-this-spec) and a new `bl` (post-this-spec) on the same repo, with `forge-pr` mode configured.

| Scenario | Old `bl` behavior | Risk | Mitigation |
|---|---|---|---|
| Read state branch with new fields (`repo.json` `integrate`, `task.target_branch`) | Silently ignores via lenient serde | None | Confirmed: `Task` has no `deny_unknown_fields` (src/task.rs:114). `repo.json` carries lenient unknown-field handling per SPEC-clone-layout §6.9. |
| `bl ready` / `bl list` on a `forge-pr`-mode repo with gate children | Sees gate children as regular open tasks | Low — a confused agent might try to claim one | SKILL.md already warns: do not claim a task that is the target of an open `gates` link. |
| `bl claim` a gate child | Succeeds; agent enters a worktree for a no-op task | Low | Same SKILL.md guidance. The plugin will close the gate child via sync-report regardless, so the worktree gets abandoned. |
| `bl review` on a parent in a `forge-pr`-mode repo | Direct-squashes into `effective_target_branch`, contaminating the integration branch with a premature squash | **Accepted caveat.** | Advisory `min_bl_version`. Documented in README and SKILL. No engineering prevention. |
| `bl close` on a `forge-pr`-mode parent before the PR merges | Refused by the existing gates check | None | This is the BC-load-bearing case. The old client behaves correctly without knowing about `forge-pr` mode. |
| `bl sync` on a `forge-pr`-mode repo | Standard git sync; the state-branch fields it doesn't understand are passed through as opaque JSON | None | Field-wise resolver operates on field names, not types; unknown fields are preserved on round-trip. |

The single accepted caveat is `bl review` by an old client. Per project decision 2026-05-10, we document and warn, do not prevent.

## 11. Hand-operable shell sequences

Mirroring SPEC-orphan-branch-state.md §11. Every `bl` operation under the new modes is expressible with stock git plus a JSON edit. A user with `vim`, `git`, and `jq` can perform `forge-pr`-mode review by hand:

```sh
# In-progress task bl-xxxx, work branch work/bl-xxxx exists.
git -C $WORKTREE add -A && git -C $WORKTREE commit -m "wip: bl-xxxx" || true
git -C $WORKTREE merge "$TARGET_BRANCH"
git -C $WORKTREE push -u origin "work/bl-xxxx"

# Auto-create gate child + link + flip parent status — three edits + one commit
# on the state branch:
GATE_ID=$(bl create "Forge: PR merged for bl-xxxx" --parent bl-xxxx --tag forge-gate)
bl link add bl-xxxx gates "$GATE_ID"
# bl update bl-xxxx status=review --note "..."   # done in one commit by bl review
```

These sequences must remain valid for the life of the design. A future change that breaks them is a breaking change to the spec.

## 12. Non-goals

- **Forge-agnostic generic plugin.** Each forge gets its own plugin. The protocol is shared; the API integration is per-forge.
- **Automatic PR creation as a core feature.** Core never opens PRs. Plugins do, or operators do via `gh pr create` / equivalent.
- **Tag enforcement on PR title at the core level.** Core *recommends* the format. Plugins *enforce* it when they create PRs. Operators creating PRs by hand can forget; the half-push detector catches the consequence.
- **Multi-branch delivery from a single task.** One task → one integration branch. Cherry-picking the same delivery to a release branch is a separate operation, not a balls-managed lifecycle.

## 13. Open questions

1. **Auto-gate child title format.** Proposed: `"Forge: PR merged for bl-xxxx"`. Should it carry the PR URL once known (via plugin push)? Probably yes — update via `bl update <gate_id> --note` carrying the URL. Specify in implementation ball.
2. **Sync-report `add_note` carrying merge SHA: format.** Free-form note string vs structured. Free-form is simpler; the parent's close just runs tag-scan and ignores the note. Defer to plugin ball.
3. **Forge plugin name in `task.external`.** GitHub: `github`. GitLab: `gitlab`. But what if a repo has both a github mirror and a gitlab primary? Punt: one forge plugin per repo for now. Revisit if it bites.
4. **`bl close` on a gate child that was opened by `bl review` in `forge-pr` mode**: should it carry semantics beyond closing the gate child task (e.g., automatic note on parent)? Initial answer: no, keep it as a plain close. Plugin sync-report carries the merge SHA via `add_note` to the parent if it wants to.

## 14. Conformance tests

These tests must exist and fail for the right reason before any implementation ball lands. Each bullet corresponds to at least one integration test.

1. Default config (no `integrate` block, no `task.target_branch`): a complete `bl claim → bl review → bl close` cycle is bit-identical to current behavior, including the resulting commits on the integration branch and the state branch.
2. With `main` checked out at repo root and `task.target_branch = "develop"`: `bl review` squashes into `develop`, not `main`.
3. With `develop` checked out at repo root and `task.target_branch = "main"`: `bl review` squashes into `main` (per-task override wins over `HEAD@root`).
4. `integrate.mode = "forge-pr"`, `task.target_branch = "main"`: `bl review` pushes `work/bl-xxxx` to origin, creates gate child with `gates` link, flips parent to `review`, does not touch `main`, does not set `delivered_in`.
5. (4), then `bl close <parent>` immediately: refused by existing gates check. Worktree intact.
6. (4), then close the gate child manually: subsequent `bl close <parent>` succeeds; `delivered_in` is null and the warning fires (no tag on main yet).
7. (4), then simulate forge merge by cherry-picking the work branch onto `main` with `[bl-xxxx]` in the subject, then close the gate child, then `bl close <parent>`: parent close populates `delivered_in` via tag-scan.
8. (4), then reject the parent (`bl update <parent> status=in_progress`): gate child is closed atomically in the same commit. Parent is `in_progress`. Work branch on origin is left alone.
9. Old-`bl` simulation: `bl close` on a `forge-pr`-mode gated parent is refused by an unmodified pre-spec binary (uses the existing gates check). Recorded as a test fixture, not a live old-binary run.
10. `bl close --delivered <sha>`: writes the given SHA into `delivered_in` regardless of tag-scan result.
11. `bl sync` on a `forge-pr`-mode repo with a plugin that emits a sync-report closing a gate child: the gate child closes, parent's close becomes unblocked, and a subsequent `bl close <parent>` populates `delivered_in` from the merge SHA on the effective target branch.
12. Half-push detection on a `forge-pr`-mode close where no tag landed on the effective target branch: detector flags the parent as half-pushed, identically to `direct` mode.

No implementation ball lands until every test in this list exists and fails for the right reason.
