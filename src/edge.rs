//! The binary edge's resolved inputs — env read once, in `main`, then handed in.
//!
//! Everything host-derived a checkout-lifecycle op needs (`bl prime`/`bl sync`,
//! §12/§13) is gathered here at the process boundary and passed as data, so the
//! library does no env reads (the bl-bfa8 rule: parallel tests vary the layout
//! without racing a shared `std::env`). [`crate::run`] takes an `&Edge`; the
//! `bl` binary builds it from `HOME`/`$XDG_*`, the current dir, `$USER`, the
//! recursion depth, and the shipped `tracker` sibling binary.

use crate::layout::Xdg;
use std::path::PathBuf;

/// The host inputs for one `bl` invocation. `invocation_path` is where `bl` was
/// run (the §7 `binding.invocation_path`); `default_actor` is the identity an op
/// uses unless `--as` overrides it; `depth` is `$BALLS_PLUGIN_DEPTH` (`0` at the
/// top level, the §6 recursion guard); `tracker_bin` is the shipped default
/// remote-talker if it is installed beside `bl` (`None` ⇒ a tracker-free,
/// stealth-only box — prime founds substrate but wires no remote, §12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge {
    pub xdg: Xdg,
    pub invocation_path: PathBuf,
    pub default_actor: String,
    pub depth: u32,
    pub tracker_bin: Option<PathBuf>,
}

impl Edge {
    /// Assemble an `Edge` from raw boundary values. `main` reads each from the
    /// environment (one always-executed line apiece) and hands them here, so the
    /// fallback logic — default actor, depth parse, sibling resolution — lives in
    /// the library where unit tests reach every branch (the bl-bfa8 rule).
    #[must_use]
    pub fn resolve(
        home: PathBuf,
        config_home: Option<String>,
        state_home: Option<String>,
        invocation_path: PathBuf,
        user: Option<String>,
        depth: Option<String>,
        current_exe: Option<PathBuf>,
    ) -> Self {
        Self {
            xdg: Xdg::with(&home, config_home.as_deref(), state_home.as_deref()),
            invocation_path,
            default_actor: user.unwrap_or_else(|| "unknown".into()),
            depth: depth.and_then(|d| d.parse().ok()).unwrap_or(0),
            tracker_bin: sibling(current_exe.as_deref(), "tracker"),
        }
    }
}

/// The path to a `name`d binary beside `current_exe`, if it exists — how the
/// shipped default plugins (the `tracker`) are found next to `bl` (§6/§12). An
/// absent exe or missing sibling ⇒ `None` (a stealth-only box).
fn sibling(current_exe: Option<&std::path::Path>, name: &str) -> Option<PathBuf> {
    let path = current_exe?.parent()?.join(name);
    path.exists().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
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
        )
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
    fn the_tracker_sibling_is_found_only_when_it_exists() {
        let tmp = TempDir::new().unwrap();
        let bl = tmp.path().join("bl");
        std::fs::write(&bl, "x").unwrap();
        // No tracker beside it yet.
        assert_eq!(resolve(None, None, Some(bl.clone())).tracker_bin, None);
        // Now the sibling exists.
        let tracker = tmp.path().join("tracker");
        std::fs::write(&tracker, "x").unwrap();
        assert_eq!(resolve(None, None, Some(bl)).tracker_bin, Some(tracker));
    }

    #[test]
    fn an_absent_exe_yields_no_tracker() {
        assert_eq!(resolve(None, None, None).tracker_bin, None);
        // A root path has no parent — also no sibling.
        assert_eq!(sibling(Some(Path::new("/")), "tracker"), None);
    }
}
