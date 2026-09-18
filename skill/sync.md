# bl sync — reconcile the store with the remote

    usage: bl sync [BRANCH] [--as ID] [--remote URL]

Reconciles the store with the remote: fetch, rebase every unpublished local
seal onto the remote tip, push. A store that is behind fast-forwards; one that
is ahead **publishes** — `bl sync` is how a store publishes when the tracker is
not wired to push on every op (`bl conf --skill`, "the plugin schedule"). No
arg syncs the configured store branch. The remote resolves the same way as every op —
`--remote` > `task-remote` > `origin` (the full ladder is in `bl prime
--skill`).

## Flags

- `--as ID` — worker identity.
- `--remote URL` — per-op store remote (not remembered). `--center` is
  prime-only (it enrolls a checkout); on `sync` it bounces as an unknown flag.
- `-C PATH` — **global** (every command): address the store keyed by PATH, as
  if `bl` had run there. No walking, no git-root discovery.

## Examples

    bl sync                # sync the configured store branch
    bl sync balls/tasks    # sync a named branch

## Notes

`bl prime` already syncs on every session start, so a standalone `bl sync` is for
pulling in a sibling's pushes mid-session, or for publishing what this store
holds. Every mutating op runs the same reconcile itself when its push is
rejected, so a race on a *different* ball never needs you: the seal is rebased
onto the remote tip and lands in the same op.

Every seal is one commit touching one `tasks/<id>.md`, so seals on different
balls rebase clean by construction. The one thing `sync` refuses is the **same
ball changed on both sides** — two claims of one ball, a close against an
update the remote took — and it refuses by name:

    push rejected: `origin`'s `balls/tasks` moved and bl-1a2b changed on both
    sides, so the rebase of this store's unpublished seals was aborted —
    nothing was published and nothing local was changed. …

The rebase is aborted, every local seal is exactly where it was, and the
remote is untouched. The exits are yours, stated in the message: for an op that
just aborted, `bl sync` then re-run it (a claim someone else already holds is
contention, and the later claim loses); for seals this store still holds,
`git -C <store> log FETCH_HEAD..<branch>` lists them — `git -C <store> rebase
FETCH_HEAD`, resolve the named file, then `bl sync` publishes — or `git -C
<store> reset --hard FETCH_HEAD` to take the remote's version. balls never
merges a ball field-wise.

An unreachable remote is not an error: `sync` (and every op) warns and leaves
the store ahead, unpublished, until the remote is back. Only a same-ball
conflict, or a push the remote denies after a clean rebase (permissions, a
server hook), aborts.
