# bl sync — pull the store from the remote

    usage: bl sync [BRANCH] [--as ID] [--remote URL]

Pulls the store from the remote (fetch + fast-forward). No arg syncs the
configured store branch. The remote resolves the same way as every op —
`--remote` > `task-remote` > `origin` (the full ladder is in `bl prime
--skill`).

## Flags

- `--as ID` — worker identity.
- `--remote URL` — per-op store remote (not remembered). `--center` is
  prime-only (it enrolls a checkout); on `sync` it bounces as an unknown flag.

## Examples

    bl sync                # sync the configured store branch
    bl sync balls/tasks    # sync a named branch

## Notes

`bl prime` already syncs on every session start, so a standalone `bl sync` is for
pulling in a sibling's pushes mid-session. If a plugin appended after
`bl-tracker` in a hook once failed and left the local seal ahead of a rejected
push, `bl sync` resurrects the seal from the remote — then retry the op.
