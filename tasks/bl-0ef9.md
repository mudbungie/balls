+++
title = "github-issues adoption: stamp the [bl-xxxx] marker, not a per-machine join store (fix bl-2a81 federation hole)"
created = 1780965724
updated = 1780966680
claimant = "Strictly"
parent = "bl-1280"
priority = 4
delivery-worktree = "/home/mark/.local/state/balls/plugins/bl-delivery/home/mark/dev/balls/bl-0ef9"
+++
Rework github-issues legacy-adoption (bl-2a81, plugin main 64a76b4) to stamp the `[bl-xxxx]` issue-title MARKER instead of seeding a per-machine join store.

## Problem
bl-2a81's `adopt` seeds the reconciliation base (`base.json`, XDG territory, PER-MACHINE) with `{ball-id -> {number,...}}` so the first greenfield `sync` joins legacy (marker-less) issues via `Base::id_for_number` -> zero dup. But the join SSOT chosen in bl-613d is the `[bl-xxxx]` marker on the GitHub issue TITLE, not the base. Seeding the base parks the join in machine-local state that:
- breaks federation (§12): a clone that never ran `adopt` has an empty base, sees a marker-less issue, and AUTO-CREATES a duplicate ball;
- is NOT rebuildable from GitHub, contradicting base.rs's own invariant ("machine-local, derived, rebuildable from GitHub ... because the marker still recovers the join") — for adopted issues there is no marker, so the base becomes the SOLE copy of the join.

Root cause: the join already has a canonical home (the marker); adoption wrote a parallel copy elsewhere. §16's literal "seed the XDG scratch from the old number" predates bl-613d's marker-is-SSOT decision and is stale.

## Fix
`adopt` amends the SSOT: for each legacy task carrying `external.github-issues.issue.number`, append `[bl-id]` to GitHub issue #number's title (one PATCH, idempotent via `marker::append`). This makes adopt ONLINE (load token from territory like `auth-check`; reuse the GitHub client). DROP the base-seed entirely — no `Snapshot` writing in adopt. The first `sync` then joins via the marker (pull priority 1, ALREADY implemented — `pull.rs` needs NO change) and rebuilds `base.json` itself.

`base.json` stays, but only as a rebuildable SYNC-CACHE (the last-agreed snapshot for push idempotency / loop-avoidance) — user-confirmed acceptable. Federation is free: the marker is on GitHub, visible to every clone. Keep the existing skips (closed = §9 absence; never-mirrored = no number) and re-run idempotency (`marker::append` no-ops an already-correct marker).

### Why the issue title, not ball frontmatter
A preserved extra (`bl update <id> gh-number=N`) is possible but worse: it reintroduces the `external.github-issues.*`-on-the-ball model bl-613d deliberately deleted; every touch becomes a commit->push under the no-return-channel sync protocol; and GitHub doesn't carry it, so it's invisible to other clones (same federation hole, relocated). The marker is the one amendment that is both the chosen SSOT and visible everywhere.

## Exit requirements
- Code: `adopt.rs` reworked; `make check` green (100% cov / clippy / 300-line cap); deployed via `make install`.
- DOCS (REQUIRED): update (a) the plugin README "Migrating from legacy balls" section (it currently describes seeding the base from numbers) AND (b) architecture §16's github-issues worked example ("Its port's one-time adoption seeds `plugins/github-issues/<id>/` (§14 scratch) from the old number, so the next `sync` re-adopts the existing issue with ZERO dup") — both must describe marker-stamping adoption + base.json as a rebuildable cache.
- Acceptance: a clone that did NOT run `adopt` joins adopted issues via the marker with zero dup — the federated-dup caveat is GONE.

Cross-repo: code in /home/mark/dev/balls-github-plugin (sibling repo), ball in balls (empty balls-side delivery), deploy = `make install`. Supersedes the federated-dup caveat recorded at bl-2a81's close (balls main, seal 71f3c0c).