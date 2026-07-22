# bl prime — ready this checkout (run at session start)

    usage: bl prime [--as ID] [--remote URL] [--center URL]
             [--install CENTER] [--stealth]

**Run this at the start of every session**, then `bl list`. `prime` is
idempotent: on first run it **founds** the local substrate (there is no separate
`bl init`) — seeding `config/` from the install defaults and creating the store
— then syncs with the remote. It also prunes the settled `work/<id>` branches
that delivered closes leave behind, and sweeps stale-read seen-tokens naming
absent task files (see `bl close --skill`). It prints no listing of its own
(worktrees materialize at `claim`, not here).

## Flags

- `--as ID` — worker identity.
- `--remote URL` — per-op store remote (shapes one op; the ladder's top tier; not
  remembered).
- `--center URL` — **enroll** this checkout into CENTER: durable per-clone bind +
  adopt its `config/` + prime, in one command. Prime-only; subsumes `--install`.
- `--install CENTER` — adopt `config/` from CENTER only (no durable bind).
- `--stealth` — opt out of any store remote, **durably**; the store stays local.
- `-C PATH` — **global** (every command): found/address the substrate keyed by
  PATH, as if `bl` had run there. `bl -C ~/dev/proj prime` founds the project's
  checkout from anywhere; `-C` inside a `work/<id>` worktree points ops back at
  the project root. No walking, no git-root discovery — the path is the address.

Founding on a miss first checks whether an ANCESTOR directory already carries a
founded store (balls' own record; git is never consulted) and, if so, warns on
stderr before founding here anyway — a report only, never a refusal or a
redirect: `founding a new store here; an existing store sits at <ancestor> —
meant that one? (cd there, or bl -C <ancestor>)`. A re-prime of an already-founded
checkout stays silent, and founding a deliberate nested/sibling store still
works exactly as before — just no longer invisibly.

## Examples

    bl prime --as alice
    bl prime --remote git@host:repo.git
    bl prime --center git@host:hub.git     # enroll into a shared center
    bl prime --center ~/hub.git            # a local bare repo is a valid center
    bl prime --stealth

## The store remote resolves the same way on every command

Highest to lowest:

1. `--remote URL` — a per-op override; **not** remembered.
2. the per-checkout **stealth sentinel** (`bl conf set task-remote none` — "no
   remote, on purpose"; resolution stops here).
3. this checkout's own `task-remote` (`bl conf set task-remote <url>` — a
   per-clone binding, ranked above a legacy machine-wide config kept only as a
   read-only fallback).
4. the project repo's `origin`.

A fresh clone whose `origin` carries the store just works: `bl prime; bl list`.
To **enroll** a checkout with no such `origin` into a shared project (a
"center"), the one-shot is `bl prime --center <hub>`: it writes the durable
per-clone binding, adopts that center's committed `config/`, and primes — one
command, no half-enrolled window. A filesystem path is a legitimate `<hub>` (two
repos on one box share through a local bare repo, the same code path as a hosted
center). The rule: **`--remote` shapes one op; `--center` enrolls a checkout** —
`--center` is prime-only, and on any other verb it bounces as an unknown flag.
`--center` subsumes `--install` (pass one or the other, mutually exclusive at
parse); `--install <hub>` adopts config *without* the durable bind. A bare
`--remote` alone shapes only that one invocation; prime **warns** when nothing
durable backs it, because every later plain command would silently run stealth.

In a repo with a pushable `origin`, prime founds a `balls/tasks` branch there and
pushes it. `bl prime --stealth` is the opt-out and is **durable**: sugar for `bl
conf set task-remote none`, a committed landing-config sentinel that every later
command derives — no op founds, pushes, or discovers anything until you set a
remote (`bl conf set task-remote <url>` clears it). It contradicts
`--remote`/`--center`/`--install` (each names a remote) and is refused at parse.

Re-running plain `bl prime` converges to a no-op. `bl conf` shows the remote and
branch a checkout actually resolves — see `bl conf --skill`.

## Plugins and priming

A plugin whose binary is not installed beside `bl` is pruned at prime, so a
remote-less or plugin-less box still works. `bl-tracker` is the component that
talks to the remote (fetch + ff on sync, push after each op, found/adopt on
prime); see `bl conf --skill` for the plugin schedule.
