//! Hardened `git` invocation — the one safety primitive both git spawn sites
//! ([`crate::git`] and [`crate::tracker::git`]) share (bl-2d6d security review).
//!
//! balls always targets an explicit checkout via `-C <cwd>`, so the `GIT_*`
//! variables that REDIRECT which repository / object-store / index git operates
//! on can only ever MISDIRECT it — a silent wrong-repo when balls runs inside an
//! ambient git context (a hook, a parent that exported `GIT_DIR`), or an
//! attacker's hijack of the process environment, which bypasses the §4
//! config-is-RCE consent boundary entirely. [`at`] strips that family before
//! every spawn. What it deliberately PRESERVES is the auth/identity inheritance a
//! legitimate fetch/push needs and which is indistinguishable from user intent:
//! `SSH_AUTH_SOCK`, `HOME` (→ `~/.gitconfig` identity + credential helpers),
//! `GIT_SSH_COMMAND`, and the proxy vars. A blanket `env_clear()` would break all
//! of those user stories; this strips only the redirection vectors.
//!
//! Two further guards close the remote-string RCE the §4 model understates:
//! `protocol.ext.allow=never` forbids git's `ext::sh -c …` transport (arbitrary
//! command execution from a remote URL), and [`reject_option_like`] refuses a
//! remote/refspec beginning with `-` so a config-sourced value cannot smuggle an
//! option (`--upload-pack=<cmd>` is itself RCE) into `fetch`/`push`.

use std::io;
use std::path::Path;
use std::process::Command;

/// `GIT_*` vars that redirect git's repo / object-store / index. balls never
/// needs one (it always passes `-C <cwd>`), so each is stripped before a spawn.
const REDIRECT_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
];

/// A `git` [`Command`] rooted at `cwd`, hardened: the [`REDIRECT_VARS`] stripped
/// from the inherited environment and the `ext::` shell transport denied. The
/// caller appends the subcommand and its args.
#[must_use]
pub(crate) fn at(cwd: &Path) -> Command {
    let mut cmd = Command::new("git");
    for var in REDIRECT_VARS {
        cmd.env_remove(var);
    }
    cmd.arg("-c").arg("protocol.ext.allow=never").arg("-C").arg(cwd);
    cmd
}

/// Refuse an untrusted positional (a remote URL or a refspec/branch) that begins
/// with `-`: git would parse it as an option, not a value, and `--upload-pack=…`
/// turns that into command execution. The guard the tracker applies to its
/// config-sourced `remote` and `tasks_branch` before `fetch`/`push`.
pub(crate) fn reject_option_like(value: &str) -> io::Result<()> {
    if value.starts_with('-') {
        return Err(io::Error::other(format!(
            "refusing git argument that looks like an option: {value:?}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_strips_every_redirect_var() {
        // `Command::get_envs` reports an `env_remove` as `(key, None)`; assert each
        // redirection var is scheduled for removal — no process-env mutation, so
        // this is race-free (the bl-bfa8/bl-ad4b lesson).
        let cmd = at(Path::new("/proj"));
        let removed: Vec<String> = cmd
            .get_envs()
            .filter(|(_, v)| v.is_none())
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        for var in REDIRECT_VARS {
            assert!(removed.iter().any(|k| k == var), "{var} not stripped");
        }
    }

    #[test]
    fn at_denies_ext_transport_and_targets_cwd() {
        let cmd = at(Path::new("/proj"));
        let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args.windows(2).any(|w| w == ["-c", "protocol.ext.allow=never"]));
        let dash_c = args.iter().position(|a| a == "-C").expect("-C is set");
        assert_eq!(args[dash_c + 1], "/proj");
    }

    #[test]
    fn ext_transport_is_actually_refused_at_runtime() {
        // The behavioural half: a built command really rejects an `ext::` remote.
        let out = at(Path::new("/")).args(["ls-remote", "ext::sh -c id"]).output().unwrap();
        assert!(!out.status.success());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("ext"), "git refused the ext transport: {err}");
    }

    #[test]
    fn reject_option_like_blocks_dash_values_only() {
        assert!(reject_option_like("--upload-pack=evil").is_err());
        assert!(reject_option_like("-x").is_err());
        assert!(reject_option_like("origin").is_ok());
        assert!(reject_option_like("git@github.com:o/r.git").is_ok());
        assert!(reject_option_like("balls/tasks").is_ok());
    }
}
