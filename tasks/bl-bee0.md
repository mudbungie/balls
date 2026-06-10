+++
title = 'conf set <hooks-key> with an empty value writes [""] and dispatch fails with Permission denied'
created = 1781067252
updated = 1781067252
parent = "bl-72a8"
priority = 3
tags = ["bug"]
+++
`bl conf set update.pre ""` is accepted and writes "update.pre" = [""] to plugins.toml; `bl conf` then displays the key as (none), but the next update aborts with `bl: plugin  aborted the op: Permission denied (os error 13)` (the empty name resolves bin/ itself and execs a directory).

Two small fixes: refuse empty plugin names at conf write (set and the list verbs), and have dispatch route an empty/unresolvable name to the clean SS6 dangling-entry error ("referenced but bin/<name> missing") instead of EACCES.