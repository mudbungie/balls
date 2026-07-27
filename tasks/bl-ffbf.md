+++
title = "Atomicity audit leftovers: binding.toml read-modify-write, the founding crash window, and chore's nested create"
created = 1785027734
updated = 1785124394
claimant = "oakum"
parent = "bl-ea55"
priority = -1
root_commit = "91c6469b14fef602e0bb5ab9957b09937623a0da"
+++
Three low-severity gaps the bl-ea55 audit turned up. Each is a real hole in 'one CAS commit point per repo per op', none is load-bearing under today's traffic; filed together so they are not lost.

1. binding.toml (src/conf_write.rs binding_set) is a read-modify-write with no CAS and no atomic replace: two concurrent `bl conf set` in one clone lose a field, and a torn write loses the file. It is the only balls-owned mutable state outside git. Minimum fix: write-temp + rename (git's index.lock discipline). Lost-update remains unless it moves under a ref.

2. Founding is not a transaction: checkout.rs guards on is_landing() = 'a config/ dir exists', but found_landing() creates the dir, seeds it, and only then commits. A crash in that window leaves a landing with no commit on balls/config — is_landing() then reports true forever and every later op fails opening a change worktree on an unborn HEAD. Fix: the predicate should be a COMMIT on the landing branch, not a directory.

3. The bl-chore plugin's claim.post shells `bl create` — a nested balls op with its own commit point in the same store, outside the parent op's atom, whose rollback is a no-op (src/chore.rs). An aborted claim therefore leaves an orphan chore child. This is the §14 appendix case (an artifact keyed to an op that never sealed, which nothing converges onto) but is not named there. Decide: delete the child on rollback like the jira example, or declare the orphan acceptable and say so in §14.