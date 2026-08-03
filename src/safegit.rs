//! Hardened `git` invocation — the safety primitives shared by core git spawn
//! sites ([`crate::git`], `tracker::git`, [`crate::message`]) and the delivery
//! plugin's stricter local-only boundary ([`delivery_at`], bl-1ec6).
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
//!
//! Delivery has no remote/auth use-case, so it can be stricter. Its environment
//! is rebuilt by [`delivery_env`]: ordinary non-`GIT_*` execution variables are
//! retained for git and project hooks, but the only inherited `GIT_*` values are
//! author/committer identity and time. Global/system config is disabled and
//! every other Git control variable is absent, including unbounded indexed
//! `GIT_CONFIG_KEY_N` / `GIT_CONFIG_VALUE_N` input and repository redirects.
//! Repository-local/worktree config still loads; it is the delivery authority.

use std::ffi::OsStr;
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

/// The complete inherited `GIT_*` allowlist for local delivery. Core exports
/// the dates as the operation instant; explicit identity is legitimate author
/// input. No Git behavior/configuration variable is needed.
const DELIVERY_IDENTITY_VARS: &[&str] = &[
    "GIT_AUTHOR_NAME",
    "GIT_AUTHOR_EMAIL",
    "GIT_AUTHOR_DATE",
    "GIT_COMMITTER_NAME",
    "GIT_COMMITTER_EMAIL",
    "GIT_COMMITTER_DATE",
];

#[cfg(unix)]
const NULL_GIT_CONFIG: &str = "/dev/null";
#[cfg(windows)]
const NULL_GIT_CONFIG: &str = "NUL";

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

/// A local-delivery `git` command rooted at `cwd`. Unlike [`at`], this rebuilds
/// the process environment through [`delivery_env`]; delivery never needs
/// ambient Git config, repository selection, object storage, or auth controls.
#[must_use]
pub(crate) fn delivery_at(cwd: &Path) -> Command {
    let mut cmd = Command::new("git");
    delivery_env(&mut cmd);
    cmd.arg("-c")
        .arg("protocol.ext.allow=never")
        .arg("-C")
        .arg(cwd);
    cmd
}

/// Rebuild `cmd`'s environment for delivery Git and the manually executed
/// pre-commit gate. Keeping all non-`GIT_*` values preserves executable lookup
/// and hook toolchains. Inside Git's namespace, only identity/time crosses the
/// boundary; safe config-search policy is then supplied explicitly. Thus the
/// local/worktree repo config remains active while system, global, command-env,
/// indexed, redirect, discovery, object/index and execution overrides cannot
/// arrive from the caller. The prefix test is case-insensitive for Windows.
pub(crate) fn delivery_env(cmd: &mut Command) {
    cmd.env_clear()
        .envs(std::env::vars_os().filter(|(key, _)| delivery_variable(key)))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", NULL_GIT_CONFIG)
        .env("GIT_ATTR_NOSYSTEM", "1");
}

/// Whether an ambient variable may cross the delivery boundary.
fn delivery_variable(key: &OsStr) -> bool {
    let bytes = key.as_encoded_bytes();
    let is_git = bytes
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"GIT_"));
    !is_git
        || DELIVERY_IDENTITY_VARS
            .iter()
            .any(|allowed| bytes.eq_ignore_ascii_case(allowed.as_bytes()))
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
    fn delivery_filter_admits_only_identity_from_the_git_namespace() {
        for allowed in DELIVERY_IDENTITY_VARS {
            assert!(
                delivery_variable(OsStr::new(allowed)),
                "{allowed} not retained"
            );
        }
        for hostile in [
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_KEY_999999",
            "GIT_CONFIG_VALUE_999999",
            "GIT_CONFIG_PARAMETERS",
            "GIT_CONFIG_GLOBAL",
            "GIT_DIR",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_CEILING_DIRECTORIES",
            "GIT_DISCOVERY_ACROSS_FILESYSTEM",
            "GIT_EXEC_PATH",
            "GIT_TEMPLATE_DIR",
            "git_work_tree",
        ] {
            assert!(!delivery_variable(OsStr::new(hostile)), "{hostile} leaked");
        }
        assert!(delivery_variable(OsStr::new("PATH")));
        assert!(delivery_variable(OsStr::new("HOME")));
    }

    #[test]
    fn delivery_command_replaces_prior_git_controls_with_safe_policy() {
        let mut cmd = Command::new("git");
        cmd.env("GIT_CONFIG_COUNT", "1")
            .env("GIT_DIR", "/elsewhere");
        delivery_env(&mut cmd);
        let envs: Vec<(String, Option<String>)> = cmd
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert!(!envs
            .iter()
            .any(|(key, _)| key == "GIT_CONFIG_COUNT" || key == "GIT_DIR"));
        assert!(envs.contains(&("GIT_CONFIG_NOSYSTEM".into(), Some("1".into()))));
        assert!(envs.contains(&("GIT_CONFIG_GLOBAL".into(), Some(NULL_GIT_CONFIG.into()))));
        assert!(envs.contains(&("GIT_ATTR_NOSYSTEM".into(), Some("1".into()))));

        let cmd = delivery_at(Path::new("/project"));
        let args: Vec<String> = cmd
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-c", "protocol.ext.allow=never"]));
        assert!(args.windows(2).any(|pair| pair == ["-C", "/project"]));
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
