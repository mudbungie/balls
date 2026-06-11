//! The binary edge's resolved inputs — env read once, in `main`, then handed in.
//!
//! Everything host-derived a checkout-lifecycle op needs (`bl prime`/`bl sync`,
//! §12/§13) is gathered here at the process boundary and passed as data, so the
//! library does no env reads (the bl-bfa8 rule: parallel tests vary the layout
//! without racing a shared `std::env`). [`crate::run`] takes an `&Edge`; the
//! `bl` binary builds it from `HOME`/`$XDG_*`, the current dir, `$USER`, the
//! recursion depth, and the directory `bl` itself lives in (where the shipped
//! sibling plugins are found).

use crate::layout::Xdg;
use std::path::PathBuf;

/// The host inputs for one `bl` invocation. `invocation_path` is where `bl` was
/// run (the §7 `binding.invocation_path`); `default_actor` is the identity an op
/// uses unless `--as` overrides it; `depth` is `$BALLS_PLUGIN_DEPTH` (`0` at the
/// top level, the §6 recursion guard); `exe_dir` is the directory holding `bl`,
/// where the shipped sibling plugins (`tracker`, `bl-delivery`) live so the seed
/// can bind them (`None` ⇒ exe path unknown — every default plugin prunes and the
/// box runs stealth/plugin-free, §12). `color`
/// is the resolved rich-output signal the read verbs honour (§9): stdout is a
/// tty AND `NO_COLOR` is unset — a tty/env fact, so it is decided here at the
/// edge (the bl-bfa8 rule), never in the render layer. `log_level` is the §4
/// layer-1 CLI override (`--log-level`): unlike the other fields it is argv- not
/// env-derived, so [`crate::run`] strips the flag and stamps it on — `resolve`
/// leaves it `None` (no override; the config threshold stands). `path_dirs`
/// is `$PATH` split into its directories — the §6 install binary-resolution
/// lookup ("PATH or explicit `--bin`") — read once here like every other env
/// fact (the bl-bfa8 rule).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub xdg: Xdg,
    pub invocation_path: PathBuf,
    pub default_actor: String,
    pub depth: u32,
    pub exe_dir: Option<PathBuf>,
    pub path_dirs: Vec<PathBuf>,
    pub color: bool,
    pub log_level: Option<String>,
}

impl Edge {
    /// Assemble an `Edge` from raw boundary values. `main` reads each from the
    /// environment (one always-executed line apiece) and hands them here, so the
    /// fallback logic — default actor, depth parse, sibling resolution, the
    /// colour decision — lives in the library where unit tests reach every branch
    /// (the bl-bfa8 rule). `no_color` is `$NO_COLOR` (any value, even empty,
    /// disables colour — the de-facto standard); `stdout_tty` is whether stdout
    /// is a terminal (a syscall `main` makes at the boundary).
    #[must_use]
    #[allow(clippy::too_many_arguments)] // a boundary adapter: each arg is one host input
    pub fn resolve(
        home: PathBuf,
        config_home: Option<String>,
        state_home: Option<String>,
        invocation_path: PathBuf,
        user: Option<String>,
        depth: Option<String>,
        current_exe: Option<PathBuf>,
        path: Option<std::ffi::OsString>,
        no_color: Option<String>,
        stdout_tty: bool,
    ) -> Self {
        Self {
            xdg: Xdg::with(&home, config_home.as_deref(), state_home.as_deref()),
            invocation_path,
            default_actor: user.unwrap_or_else(|| "unknown".into()),
            depth: depth.and_then(|d| d.parse().ok()).unwrap_or(0),
            exe_dir: current_exe.and_then(|e| e.parent().map(std::path::Path::to_path_buf)),
            path_dirs: path.map(|p| std::env::split_paths(&p).collect()).unwrap_or_default(),
            color: stdout_tty && no_color.is_none(),
            log_level: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn resolve(user: Option<&str>, depth: Option<&str>, exe: Option<PathBuf>) -> Edge {
        Edge::resolve(
            PathBuf::from("/home/x"),
            None,
            Some("/state".into()),
            PathBuf::from("/proj"),
            user.map(str::to_string),
            depth.map(str::to_string),
            exe,
            None,
            None,
            true,
        )
    }

    /// Build an edge varying only the two colour inputs.
    fn color_of(no_color: Option<&str>, stdout_tty: bool) -> bool {
        Edge::resolve(
            PathBuf::from("/h"),
            None,
            None,
            PathBuf::from("/p"),
            None,
            None,
            None,
            None,
            no_color.map(str::to_string),
            stdout_tty,
        )
        .color
    }

    #[test]
    fn path_dirs_split_the_path_variable_and_default_empty() {
        let e = Edge::resolve(
            PathBuf::from("/h"),
            None,
            None,
            PathBuf::from("/p"),
            None,
            None,
            None,
            Some("/usr/bin:/opt/bl".into()),
            None,
            false,
        );
        assert_eq!(e.path_dirs, [PathBuf::from("/usr/bin"), PathBuf::from("/opt/bl")]);
        assert!(resolve(None, None, None).path_dirs.is_empty()); // no $PATH ⇒ no lookup dirs
    }

    #[test]
    fn an_absent_user_falls_back_to_unknown() {
        assert_eq!(resolve(Some("me"), None, None).default_actor, "me");
        assert_eq!(resolve(None, None, None).default_actor, "unknown");
    }

    #[test]
    fn depth_parses_or_defaults_to_zero() {
        assert_eq!(resolve(None, Some("3"), None).depth, 3);
        assert_eq!(resolve(None, Some("nope"), None).depth, 0); // unparseable
        assert_eq!(resolve(None, None, None).depth, 0); // absent
    }

    #[test]
    fn the_exe_dir_is_the_parent_of_the_running_binary() {
        let tmp = TempDir::new().unwrap();
        let bl = tmp.path().join("bl");
        // exe_dir is where the shipped siblings (tracker/bl-delivery) live.
        assert_eq!(resolve(None, None, Some(bl)).exe_dir.as_deref(), Some(tmp.path()));
    }

    #[test]
    fn an_absent_exe_yields_no_exe_dir() {
        assert_eq!(resolve(None, None, None).exe_dir, None);
        // A root path has no parent — also no exe dir.
        assert_eq!(resolve(None, None, Some(PathBuf::from("/"))).exe_dir, None);
    }

    #[test]
    fn colour_needs_both_a_tty_and_an_unset_no_color() {
        assert!(color_of(None, true)); // tty, NO_COLOR unset ⇒ colour
        assert!(!color_of(None, false)); // not a tty ⇒ plain
        assert!(!color_of(Some("1"), true)); // NO_COLOR set ⇒ plain
        assert!(!color_of(Some(""), true)); // even empty NO_COLOR disables
    }
}
