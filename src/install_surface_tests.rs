//! §6 install-surface conformance: "Defaults: `--to` the landing, `--from` the
//! configured upstream" and the local binary resolution "(PATH or explicit
//! `--bin <name>=<path>`)" — driven end to end on a founded substrate with the
//! [`super::tests`] fixtures (bl-4c45).

#![cfg(unix)]

use super::tests::{edge, found, g, head, run_install};
use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

/// A `side` branch of the landing whose committed `rel` carries `body` — the
/// upstream-shaped local ref the default-`--from` tests adopt from.
fn side_with(tmp: &TempDir, landing: &Path, rel: &str, body: &str) {
    let wt = tmp.path().join("side-wt");
    g(landing, &["branch", "side"]);
    g(landing, &["worktree", "add", "-q", &wt.to_string_lossy(), "side"]);
    fs::write(wt.join(rel), body).unwrap();
    g(&wt, &["add", "-A"]);
    g(&wt, &["commit", "-q", "-m", "side"]);
}

/// Write an executable fake plugin: answers `protocol` with `ops`, swallows the
/// §7 payload, then runs `hook_body` for any real `<op> <phase>` call.
fn fake_plugin(dir: &Path, name: &str, ops: &str, hook_body: &str) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    let body = format!(
        "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{{\"protocol\":[1],\"ops\":{ops}}}'; exit 0; fi\ncat >/dev/null\n{hook_body}\nexit 0\n"
    );
    fs::write(&path, body).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn a_bare_install_adopts_the_fetched_upstream_config() {
    // §6: bare `bl install` = config from the CONFIGURED UPSTREAM — the ref the
    // install.pre fetch leaves at FETCH_HEAD (primed by hand here; the chain is
    // empty on a plugin-free box).
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    side_with(&tmp, &landing, "config/balls.toml", "tasks_branch = \"balls/elsewhere\"\n");
    g(&landing, &["fetch", "-q", ".", "side"]);

    run_install(&e, &[]).unwrap();

    let cfg = fs::read_to_string(landing.join("config/balls.toml")).unwrap();
    assert!(cfg.contains("balls/elsewhere"), "adopted the upstream config: {cfg}");
}

#[test]
fn a_bare_install_with_no_upstream_is_refused_naming_the_remedy() {
    // No FETCH_HEAD (stealth box, hub without a balls/config, no fetch plugin):
    // a clean point-of-use refusal, never a raw git fatal — and nothing seals.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    let before = head(&landing);
    let err = run_install(&e, &[]).unwrap_err();
    assert!(err.to_string().contains("pass --from <ref>"), "{err}");
    assert_eq!(before, head(&landing), "nothing sealed");
}

#[test]
fn the_default_from_fetch_rides_the_install_pre_chain() {
    // The configured-upstream fetch is the TRACKER's (§0: core talks to no
    // remote): a fake tracker wired on install.pre "fetches" the side branch,
    // and the bare install stages FROM the ref that fetch leaves — so the
    // chain demonstrably runs BEFORE the engine stages.
    let tmp = TempDir::new().unwrap();
    let all_ops = r#"["sync","prime","install","claim","unclaim","close","create","update","import"]"#;
    let fetcher = "if [ \"$1\" = install ] && [ \"$2\" = pre ]; then git fetch -q . side; fi";
    fake_plugin(&tmp.path().join("bin"), "bl-tracker", all_ops, fetcher);
    let e = edge(&tmp, Some(tmp.path().join("bin")));
    let (landing, _store) = found(&e);
    side_with(&tmp, &landing, "config/balls.toml", "tasks_branch = \"balls/elsewhere\"\n");

    run_install(&e, &[]).unwrap();

    let cfg = fs::read_to_string(landing.join("config/balls.toml")).unwrap();
    assert!(cfg.contains("balls/elsewhere"), "staged from the chain's fetch: {cfg}");
}

#[test]
fn an_explicit_bin_resolves_a_referenced_plugin() {
    // §6: "PATH or explicit `--bin <name>=<path>`" — the explicit override
    // binds a referenced plugin from anywhere on this machine, validated.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    side_with(&tmp, &landing, "config/plugins.toml", "[hooks]\n\"claim.post\" = [\"myplug\"]\n");
    let bin = fake_plugin(&tmp.path().join("elsewhere"), "myplug", r#"["claim"]"#, "");

    run_install(&e, &["config", "--from", "side", "--bin", &format!("myplug={}", bin.display())]).unwrap();

    let link = landing.join("config/plugins/bin/myplug");
    assert_eq!(fs::read_link(link).unwrap(), bin);
}

#[test]
fn a_bin_failing_validation_aborts_after_the_seal_lands() {
    // The DECIDED ordering (bl-4c45): the copy is committed text (the §6
    // recommendation), binding is this box's LOCAL resolution of it — so a
    // validation refusal lands AFTER the seal, leaving the schedule committed
    // with `bin/<name>` dangling (§6's clean "referenced but not installed"
    // state). The commit is the undo; a retry with a fixed binary converges on
    // the no-op seal and just binds (§14).
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    side_with(&tmp, &landing, "config/plugins.toml", "[hooks]\n\"claim.post\" = [\"myplug\"]\n");
    let bin = fake_plugin(&tmp.path().join("elsewhere"), "myplug", r#"["close"]"#, "");
    let before = head(&landing);

    let err = run_install(&e, &["config", "--from", "side", "--bin", &format!("myplug={}", bin.display())]).unwrap_err();

    assert!(err.to_string().contains("does not handle op 'claim'"), "{err}");
    assert_ne!(before, head(&landing), "the copy sealed before the bind refusal");
    assert!(!landing.join("config/plugins/bin/myplug").exists(), "nothing bound");
}

#[test]
fn a_bin_naming_an_unreferenced_plugin_is_refused() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    side_with(&tmp, &landing, "config/balls.toml", "tasks_branch = \"balls/elsewhere\"\n");
    let err = run_install(&e, &["config", "--from", "side", "--bin", "ghost=/nope"]).unwrap_err();
    assert!(err.to_string().contains("does not reference"), "{err}");
}

#[test]
fn a_referenced_plugin_resolves_on_path_when_not_beside_bl() {
    // §6: resolution is against THIS MACHINE — a referenced plugin found on
    // PATH (edge.path_dirs) binds without `--bin` and without an exe sibling.
    let tmp = TempDir::new().unwrap();
    let pbin = tmp.path().join("pathbin");
    let bin = fake_plugin(&pbin, "pathplug", r#"["claim"]"#, "");
    let e = Edge { path_dirs: vec![pbin], ..edge(&tmp, None) };
    let (landing, _store) = found(&e);
    side_with(&tmp, &landing, "config/plugins.toml", "[hooks]\n\"claim.post\" = [\"pathplug\"]\n");

    run_install(&e, &["config", "--from", "side"]).unwrap();

    let link = landing.join("config/plugins/bin/pathplug");
    assert_eq!(fs::read_link(link).unwrap(), bin);
}

#[test]
fn locate_prefers_the_bl_sibling_then_path_then_dangles() {
    let tmp = TempDir::new().unwrap();
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    fs::create_dir_all(&a).unwrap();
    fs::create_dir_all(&b).unwrap();
    fs::write(b.join("plug"), "x").unwrap();
    let e = Edge { exe_dir: Some(a.clone()), path_dirs: vec![b.clone()], ..edge(&tmp, None) };
    assert_eq!(locate("plug", &e), Some(b.join("plug")), "PATH hit when no sibling");
    fs::write(a.join("plug"), "x").unwrap();
    assert_eq!(locate("plug", &e), Some(a.join("plug")), "the bl sibling outranks PATH");
    assert_eq!(locate("ghost", &e), None, "no hit anywhere stays dangling");
}
