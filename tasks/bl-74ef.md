+++
title = "Harden git invocation env + arg-injection guard; depth-cap fail+rollback; bounded plugin stderr; mirror-path traversal guard [bl-2d6d]"
created = 1780978931
updated = 1780978931
parent = "bl-72a8"
priority = 1
tags = ["security"]
+++
Remediations from the bl-2d6d security review (docs/security-review-subprocess-git-paths.md), core only:

HIGH-1: route both git spawn sites (src/git.rs run, src/tracker/git.rs git) through a shared hardened constructor that (a) env_remove's the repo-redirection GIT_* family (GIT_DIR/GIT_WORK_TREE/GIT_INDEX_FILE/GIT_OBJECT_DIRECTORY/GIT_ALTERNATE_OBJECT_DIRECTORIES/GIT_COMMON_DIR/GIT_NAMESPACE) — never needed since balls always uses -C <cwd>, and the silent-wrong-repo / ambient-hijack vector; (b) pins -c protocol.ext.allow=never to kill the ext::sh -c remote-RCE; auth env (SSH_AUTH_SOCK, HOME/~/.gitconfig, GIT_SSH_COMMAND, proxies) is PRESERVED. Plus reject remote/tasks_branch beginning with '-' (option-injection, e.g. --upload-pack=) in remote_ops.rs.

MED-1: implement frozen bl-7110 (architecture.md:1363) — at BALLS_PLUGIN_DEPTH cap, Subprocess::run returns Err (abort + reverse-order rollback) with a diagnostic naming op/plugin, instead of the current silent run-plugin-free (which can skip a delivery gate).

LOW-1: bound plugin.rs relay per-line read (very generous cap, ~1 MiB) so a no-newline gigabyte stderr can't OOM the parent before log.rs trims to 4096.

LOW-2: at the delivery edge, reject an invocation_path that is non-absolute or contains a '..' component before it is mirrored into the worktree territory (delivery.rs binding_territory gives up percent-encoding's ..-neutralization).

Security regression tests for each.