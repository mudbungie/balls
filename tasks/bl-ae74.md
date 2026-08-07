+++
title = "rename detection can swallow a deletion: a closed ball vanishes from show and list -s closed entirely"
created = 1785997273
updated = 1786075114
claimant = "Jinns"
priority = 3
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
tags = ["bug"]

[[blockers]]
id = "bl-4f87"
on = "close"
+++
Both dead-ball reads run `git log --diff-filter=D` WITHOUT `--no-renames`, and
git's rename detection has been on by default since 2.9. If one commit both
deletes `tasks/<a>.md` and adds a `tasks/<b>.md` that is >=50% similar, git
reports the pair as a single R, not a D + an A — so the deletion is invisible to
`--diff-filter=D` and ball `<a>` drops out of the dead set completely.

Reproduced (scratch repo, one commit deleting tasks/bl-1.md and adding a
7-of-8-lines-identical tasks/bl-2.md):

    $ git log --diff-filter=D --format= --name-only -- tasks
    (nothing)
    $ git log --diff-filter=D --format= --name-only --no-renames -- tasks
    tasks/bl-1.md

Consequence: `bl show bl-1` reports the id as naming nothing — live OR dead —
and `bl list -s closed` omits the row. Not a wrong answer but a MISSING one,
which is the worse failure: absence is how balls spells "resolved", so a
swallowed deletion is indistinguishable from a ball that never existed.

Task files are highly self-similar (identical frontmatter keys, similar bodies),
so the similarity threshold is easy to cross — the scarce ingredient is a single
commit carrying both an add and a delete under `tasks/`. Core's own ops never
mint one (create and close are separate commits; a multi-child epic close
deletes only), so this is LATENT, not observed. It becomes reachable through a
hand-edited store commit, an `import` batched with a close, or any future op that
writes and retires in one seal — i.e. exactly the paths nobody would think to
re-verify against.

The fix is one flag on both call sites in `src/reads/history.rs`
(`resolve_dead`'s `log -1`, and `newest_deletions`' enumeration): `--no-renames`
makes a deletion report as a deletion unconditionally. It WIDENS the dead set,
so it is a semantic change, not a pure refactor — hence its own ball rather than
a ride-along on bl-4c08, which deliberately preserved the existing set exactly.
Worth checking whether any other `--diff-filter` read in the tree has the same
hole.

Found while batching the dead-set reconstruction (bl-4c08, 2026-08-05).