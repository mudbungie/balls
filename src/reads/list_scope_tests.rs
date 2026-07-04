//! Root-aware `bl list` scope + fleet-view label rendering (bl-0161 Q2,
//! bl-5965) — a nested child of [`super`] (the `list` render tests), inheriting
//! its fixtures (`NOW`, `nostore`, `nogit_xdg`, `flags`, `plain`, `render`,
//! `catalog`, `task`, `Ctx`, `render_list`, …) through `use super::*`.

use super::*;
use crate::encoding::percent_encode;
use crate::reads::test_support::git_checkout;
use tempfile::TempDir;

/// Plain flags with `--everywhere` set (the fleet view).
fn everywhere() -> Flags {
    Flags { plain: true, everywhere: true, ..Default::default() }
}

/// A ready ball stamped with a project root (the create-time repo identity).
fn rooted(title: &str, created: i64, root: &str) -> Task {
    Task { root_commit: Some(root.into()), ..task(title, created) }
}

#[test]
fn default_scope_shows_this_project_and_rootless_hiding_foreign() {
    // list's default set IS the claim-admitted set: this checkout's own root plus
    // rootless balls, foreign roots hidden — the same predicate `claim` enforces.
    let tmp = TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    let r1 = git_checkout(&proj, "one");
    let cat = catalog(&[
        ("bl-home", rooted("Home", 1, &r1)),
        ("bl-away", rooted("Away", 2, "deadbeefdeadbeef")),
        ("bl-free", task("Rootless", 3)),
    ]);
    let xdg = nogit_xdg();
    let c = Ctx { store: nostore(), now: NOW, invocation: &proj, xdg: &xdg };
    let out = render_list(&cat, &[], &flags(false), &plain(), &c).unwrap();
    assert!(out.contains("bl-home") && out.contains("bl-free"), "this project + rootless: {out}");
    assert!(!out.contains("bl-away"), "foreign root hidden by default: {out}");
}

#[test]
fn everywhere_reveals_foreign_rows_with_a_short_hash_label() {
    let tmp = TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    let r1 = git_checkout(&proj, "one");
    let foreign = "deadbeefdeadbeef0000";
    let cat = catalog(&[("bl-home", rooted("Home", 1, &r1)), ("bl-away", rooted("Away", 2, foreign))]);
    let xdg = nogit_xdg(); // no clones dir → no basename can be earned
    let c = Ctx { store: nostore(), now: NOW, invocation: &proj, xdg: &xdg };
    let out = render_list(&cat, &[], &everywhere(), &plain(), &c).unwrap();
    // The foreign row is now visible and carries the short (8-char) root hash;
    // the home row stays bare (no label for this project).
    assert!(out.contains(&format!("bl-away  Away  [{}]", &foreign[..8])), "hash label: {out}");
    assert!(out.contains("bl-home  Home\n"), "home row unlabeled: {out}");
}

#[test]
fn everywhere_labels_a_foreign_row_with_an_enrolled_checkouts_basename() {
    let tmp = TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    git_checkout(&proj, "one");
    // A second checkout this box has primed, rooted differently.
    let other = tmp.path().join("other-app");
    let r2 = git_checkout(&other, "two");
    let xdg = Xdg::with(&tmp.path().join("home"), None, Some(tmp.path().join("state").to_str().unwrap()));
    // The XDG clone entry `prime` leaves — its NAME decodes back to the checkout.
    let entry = xdg.clones_dir().join(percent_encode(&other.to_string_lossy()));
    std::fs::create_dir_all(&entry).unwrap();
    let cat = catalog(&[("bl-away", rooted("Away", 1, &r2))]);
    let c = Ctx { store: nostore(), now: NOW, invocation: &proj, xdg: &xdg };
    let out = render_list(&cat, &[], &everywhere(), &plain(), &c).unwrap();
    // The foreign root matches the enrolled checkout → its directory basename
    // shadows the hash (pure render-time sugar).
    assert_eq!(out, "ready    bl-away  Away  [other-app]\n");
}

#[test]
fn a_rootless_checkout_sees_every_project() {
    // A non-git invocation has no roots, so `admits` fails open — every
    // foreign-rooted ball is visible WITHOUT `--everywhere` (single-store parity).
    let cat = catalog(&[("bl-a", rooted("A", 1, "aaaaaaaa")), ("bl-b", rooted("B", 2, "bbbbbbbb"))]);
    let out = render(&cat, &[], &flags(false), &plain());
    assert!(out.contains("bl-a") && out.contains("bl-b"), "rootless checkout sees all: {out}");
}

#[test]
fn a_rootless_catalog_is_never_scoped_against_the_checkout() {
    // Every ball is rootless, so `needs_roots` is false: even against a real git
    // checkout with a root, the set is left whole (the walk is never paid).
    let tmp = TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    git_checkout(&proj, "one");
    let cat = catalog(&[("bl-1", task("One", 1)), ("bl-2", task("Two", 2))]);
    let xdg = nogit_xdg();
    let c = Ctx { store: nostore(), now: NOW, invocation: &proj, xdg: &xdg };
    let out = render_list(&cat, &[], &flags(false), &plain(), &c).unwrap();
    assert_eq!(out, "ready    bl-1  One\nready    bl-2  Two\n");
}

#[test]
fn json_is_scope_aware_and_carries_no_label_field() {
    let tmp = TempDir::new().unwrap();
    let proj = tmp.path().join("proj");
    let r1 = git_checkout(&proj, "one");
    let foreign = "deadbeefdeadbeef";
    let cat = catalog(&[("bl-home", rooted("Home", 1, &r1)), ("bl-away", rooted("Away", 2, foreign))]);
    let xdg = nogit_xdg();
    let c = Ctx { store: nostore(), now: NOW, invocation: &proj, xdg: &xdg };

    // Default --json is scoped exactly like the human view: only the home ball.
    let scoped_f = Flags { json: true, plain: true, ..Default::default() };
    let scoped: serde_json::Value =
        serde_json::from_str(&render_list(&cat, &[], &scoped_f, &plain(), &c).unwrap()).unwrap();
    let ids: Vec<&str> = scoped.as_array().unwrap().iter().map(|v| v["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["bl-home"], "default json is scoped");

    // --everywhere --json is the whole fleet AND byte-identical to today's
    // unscoped `bl list --json`: the bedrock projection, no label field anywhere.
    let ev_f = Flags { json: true, plain: true, everywhere: true, ..Default::default() };
    let ev: serde_json::Value =
        serde_json::from_str(&render_list(&cat, &[], &ev_f, &plain(), &c).unwrap()).unwrap();
    let rows = ev.as_array().unwrap();
    assert_eq!(rows.len(), 2, "fleet json carries every ball");
    for row in rows {
        assert!(row.get("label").is_none() && row.get("project").is_none(), "no label in json: {row}");
    }
    // The foreign row's bytes are exactly the bedrock projection (no sugar).
    assert_eq!(ev[1], task_json("bl-away", &cat.get("bl-away").unwrap().task));
}
