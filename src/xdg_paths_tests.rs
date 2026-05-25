//! Tests for the XDG path layer. Pure path arithmetic — no I/O.
//! Frozen path shapes lock in SPEC §3 conformance test §14.11
//! (materialization is idempotent — same inputs, same paths).

use super::*;
use std::path::{Path, PathBuf};

fn bases() -> XdgBases {
    XdgBases::with(
        Path::new("/home/u"),
        None, None, None,
    )
}

#[test]
fn default_bases_use_xdg_spec_fallbacks() {
    let b = bases();
    assert_eq!(b.config_home, PathBuf::from("/home/u/.config"));
    assert_eq!(b.state_home, PathBuf::from("/home/u/.local/state"));
    assert_eq!(b.cache_home, PathBuf::from("/home/u/.cache"));
}

#[test]
fn explicit_xdg_overrides_defaults() {
    let b = XdgBases::with(
        Path::new("/home/u"),
        Some("/cfg"),
        Some("/state"),
        Some("/cache"),
    );
    assert_eq!(b.config_home, PathBuf::from("/cfg"));
    assert_eq!(b.state_home, PathBuf::from("/state"));
    assert_eq!(b.cache_home, PathBuf::from("/cache"));
}

#[test]
fn empty_xdg_var_falls_back_to_default() {
    // An empty `$XDG_*` is per-XDG-spec the "unset" sentinel.
    let b = XdgBases::with(
        Path::new("/home/u"),
        Some(""),
        Some(""),
        Some(""),
    );
    assert_eq!(b.config_home, PathBuf::from("/home/u/.config"));
}

#[test]
fn from_env_returns_some_when_home_set() {
    // Read-only: don't mutate env (bl-bfa8 parallel-test race).
    // Cargo always sets HOME, so this exercises the `Some` branch
    // of `from_env`'s filter+map without env mutation.
    assert!(XdgBases::from_env().is_some());
}

#[test]
fn from_strings_none_home_returns_none() {
    assert!(XdgBases::from_strings(None, None, None, None).is_none());
}

#[test]
fn from_strings_empty_home_returns_none() {
    assert!(XdgBases::from_strings(Some(String::new()), None, None, None).is_none());
}

#[test]
fn from_strings_some_home_returns_some_with_xdg_defaults() {
    let b = XdgBases::from_strings(Some("/h".into()), None, None, None).unwrap();
    assert_eq!(b.config_home, PathBuf::from("/h/.config"));
    assert_eq!(b.state_home, PathBuf::from("/h/.local/state"));
    assert_eq!(b.cache_home, PathBuf::from("/h/.cache"));
}

#[test]
fn from_strings_explicit_xdg_overrides() {
    let b = XdgBases::from_strings(
        Some("/h".into()),
        Some("/c".into()),
        Some("/s".into()),
        Some("/ca".into()),
    )
    .unwrap();
    assert_eq!(b.config_home, PathBuf::from("/c"));
    assert_eq!(b.state_home, PathBuf::from("/s"));
    assert_eq!(b.cache_home, PathBuf::from("/ca"));
}

#[test]
fn state_root_is_xdg_state_home_slash_balls() {
    assert_eq!(bases().state_root(), PathBuf::from("/home/u/.local/state/balls"));
}

#[test]
fn config_root_is_xdg_config_home_slash_balls() {
    assert_eq!(bases().config_root(), PathBuf::from("/home/u/.config/balls"));
}

#[test]
fn cache_root_reserved_but_present() {
    // SPEC §3: reserved-but-empty in this layout. The base resolves
    // even though no module under bl-77cb writes to it.
    assert_eq!(bases().cache_root(), PathBuf::from("/home/u/.cache/balls"));
}

#[test]
fn tracker_checkout_uses_flat_trackers_subdir() {
    // Frozen shape: `trackers/<enc-origin>/<enc-branch>/`. No
    // `<origin-key>` parent — that was the previous (hashed)
    // revision.
    let p = tracker_checkout(&bases(), "github.com%3Afoo%2Fbar", ENC_BALLS_TASKS);
    assert_eq!(
        p,
        PathBuf::from(
            "/home/u/.local/state/balls/trackers/github.com%3Afoo%2Fbar/balls%2Ftasks"
        )
    );
}

#[test]
fn own_tracker_checkout_uses_bootstrap_branch_constant() {
    let direct = tracker_checkout(&bases(), "github.com%3Afoo", ENC_BALLS_TASKS);
    let own = own_tracker_checkout(&bases(), "github.com%3Afoo");
    assert_eq!(direct, own);
}

#[test]
fn per_clone_paths_share_nested_suffix_under_four_sibling_dirs() {
    // Frozen shape per §3: four flat sibling dirs under state_root,
    // each carrying the same `<nested-clone-path>` suffix. No
    // shared `<origin-key>` parent (that was bl-fc50).
    let nested = PathBuf::from("home/mark/dev/balls");
    let p = PerClonePaths::new(&bases(), &nested);
    assert_eq!(
        p.worktrees,
        PathBuf::from("/home/u/.local/state/balls/worktrees/home/mark/dev/balls")
    );
    assert_eq!(
        p.claims,
        PathBuf::from("/home/u/.local/state/balls/claims/home/mark/dev/balls")
    );
    assert_eq!(
        p.locks,
        PathBuf::from("/home/u/.local/state/balls/locks/home/mark/dev/balls")
    );
    assert_eq!(
        p.plugins_auth,
        PathBuf::from("/home/u/.local/state/balls/plugins-auth/home/mark/dev/balls")
    );
}

#[test]
fn per_clone_worktree_for_appends_bl_id() {
    let p = PerClonePaths::new(&bases(), &PathBuf::from("home/mark/dev/balls"));
    assert_eq!(
        p.worktree_for("bl-abcd"),
        PathBuf::from("/home/u/.local/state/balls/worktrees/home/mark/dev/balls/bl-abcd")
    );
}

#[test]
fn clone_json_lives_under_config_root_nested() {
    let p = clone_json_path(&bases(), &PathBuf::from("home/mark/dev/balls"));
    assert_eq!(
        p,
        PathBuf::from("/home/u/.config/balls/home/mark/dev/balls/clone.json")
    );
}

#[test]
fn two_clones_of_same_origin_share_trackers_but_disjoin_per_clone() {
    // SPEC §3 / §4 / §14.3: per-tracker state collides (same
    // <enc-origin>); per-clone state separates (different
    // <nested-clone-path>).
    let b = bases();
    let enc_origin = "github.com%3Afoo%2Fbar";
    let tracker = own_tracker_checkout(&b, enc_origin);

    let p1 = PerClonePaths::new(&b, &PathBuf::from("home/mark/dev/balls-a"));
    let p2 = PerClonePaths::new(&b, &PathBuf::from("home/mark/dev/balls-b"));

    // Shared tracker checkout (one fetch refreshes both):
    let t_via_first = own_tracker_checkout(&b, enc_origin);
    let t_via_second = own_tracker_checkout(&b, enc_origin);
    assert_eq!(tracker, t_via_first);
    assert_eq!(t_via_first, t_via_second);

    // Disjoint per-clone state:
    assert_ne!(p1.worktrees, p2.worktrees);
    assert_ne!(p1.claims, p2.claims);
    assert_ne!(p1.locks, p2.locks);
    assert_ne!(p1.plugins_auth, p2.plugins_auth);
}

#[test]
fn xdg_bases_eq() {
    // Trait derive — bases compare structurally so callers can
    // assert equality between two derivations of the same env.
    assert_eq!(bases(), bases());
    assert_ne!(
        bases(),
        XdgBases::with(Path::new("/other"), None, None, None)
    );
}
