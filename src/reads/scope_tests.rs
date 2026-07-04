//! Root-aware scope + fleet-view label unit tests (bl-0161 Q2, bl-5965).

use super::*;
use crate::encoding::percent_encode;
use crate::layout::Xdg;
use crate::reads::test_support::git_checkout;
use tempfile::TempDir;

/// An [`Xdg`] with its state under `tmp/state` — its `clones/` dir is where the
/// fleet view enumerates enrolled checkouts.
fn xdg_at(tmp: &TempDir) -> Xdg {
    Xdg::with(&tmp.path().join("home"), None, Some(tmp.path().join("state").to_str().unwrap()))
}

#[test]
fn checkout_roots_is_read_only_when_needed() {
    // The lazy gate: `needed = false` (no rooted ball) skips the git walk
    // ENTIRELY — even against a real checkout whose root is non-empty — while
    // `needed = true` reads it. A rootless-only catalog therefore never shells git.
    let tmp = TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    let root = git_checkout(&proj, "one");
    assert!(checkout_roots(&proj, false).is_empty(), "needed=false must not walk");
    assert_eq!(checkout_roots(&proj, true), vec![root], "needed=true reads the root");
    // A non-git checkout fails open to an empty set (the guard withholds nothing).
    assert!(checkout_roots(&tmp.path().join("nope"), true).is_empty());
}

#[test]
fn is_foreign_is_exactly_a_recorded_root_this_checkout_refuses() {
    let roots = vec!["aaa".to_string()];
    assert!(!is_foreign(None, &roots)); // rootless ball: home everywhere
    assert!(!is_foreign(Some("aaa"), &roots)); // same root: home
    assert!(is_foreign(Some("bbb"), &roots)); // other project's root: foreign
    assert!(!is_foreign(Some("bbb"), &[])); // rootless checkout admits everything
}

#[test]
fn row_label_is_empty_off_the_fleet_view_or_for_a_home_row() {
    let tmp = TempDir::new().unwrap();
    let labels = enrolled_labels(&xdg_at(&tmp)); // empty — no clones dir exists
    let roots = vec!["aaa".to_string()];
    // No labels object at all (not `--everywhere`) → never a suffix.
    assert_eq!(row_label(None, Some("bbb"), &roots), "");
    // With labels in play, a rootless row and a home row still earn nothing.
    assert_eq!(row_label(Some(&labels), None, &roots), "");
    assert_eq!(row_label(Some(&labels), Some("aaa"), &roots), "");
    // A foreign row with no enrolled match → the short (8-char) hash suffix.
    assert_eq!(row_label(Some(&labels), Some("deadbeefcafe"), &roots), "  [deadbeef]");
}

#[test]
fn enrolled_labels_names_a_foreign_root_by_its_checkout_basename() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("widget");
    let root = git_checkout(&repo, "w");
    let xdg = xdg_at(&tmp);
    let clones = xdg.clones_dir();
    fs::create_dir_all(&clones).unwrap();
    // A live enrolled entry (rooted at `root`); one whose checkout was since
    // removed; one root-path entry with no basename; one undecodable name. Only
    // the first contributes a label — the rest are skipped silently.
    fs::create_dir_all(clones.join(percent_encode(&repo.to_string_lossy()))).unwrap();
    fs::create_dir_all(clones.join(percent_encode("/gone/checkout"))).unwrap();
    fs::create_dir_all(clones.join(percent_encode("/"))).unwrap();
    fs::create_dir_all(clones.join("not%decodable")).unwrap();
    let labels = enrolled_labels(&xdg);
    // A checkout rooted elsewhere makes both roots below foreign, so `row_label`
    // renders: the enrolled root as its basename, an unknown root as the hash.
    let elsewhere = vec!["ffffffff".to_string()];
    assert_eq!(row_label(Some(&labels), Some(&root), &elsewhere), "  [widget]");
    assert_eq!(row_label(Some(&labels), Some("0123456789ab"), &elsewhere), "  [01234567]");
}
