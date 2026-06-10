//! Tests for the §6 read-op plugin dispatch: the fold is best-effort — every
//! failure mode renders an empty contribution (the read still prints, minus the
//! line) — and a wired plugin's captured stdout comes back verbatim, carrying
//! the §7 read wire (bare-key hook, `read` phase, `metadata.bl-id`).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::fold;
use crate::edge::Edge;
use crate::layout::Xdg;
use crate::plugin::DEPTH_CAP;
use crate::registry::Registry;
use crate::verb::Verb;

/// An [`Edge`] rooted in `tmp`: XDG state under `tmp/state`, the project at
/// `tmp/proj`, no colour, top-level depth unless overridden.
fn edge(tmp: &Path, depth: u32) -> Edge {
    Edge {
        xdg: Xdg::with(&tmp.join("home"), None, Some(tmp.join("state").to_str().unwrap())),
        invocation_path: tmp.join("proj"),
        default_actor: "me".into(),
        depth,
        exe_dir: None,
        color: false,
        log_level: None,
    }
}

/// The landing dir for `edge`'s project, with `config/plugins.toml` written.
fn landing_with(edge: &Edge, plugins_toml: &str) -> PathBuf {
    let landing = edge.xdg.clone_dir(&edge.invocation_path).landing();
    fs::create_dir_all(landing.join("config")).unwrap();
    fs::write(landing.join("config").join("plugins.toml"), plugins_toml).unwrap();
    landing
}

/// Drop an executable `script` named `name` in `tmp` and bind it on `landing`.
fn bind_script(tmp: &Path, landing: &Path, name: &str, script: &str) {
    let bin = tmp.join(name);
    fs::write(&bin, script).unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    Registry::at(landing).bind(name, &bin).unwrap();
}

const SHOW_WIRED: &str = "[hooks]\n\"show\" = [\"fake\"]\n";

#[test]
fn fold_captures_a_wired_plugins_stdout_verbatim() {
    let tmp = TempDir::new().unwrap();
    let edge = edge(tmp.path(), 0);
    let landing = landing_with(&edge, SHOW_WIRED);
    // The script proves the §6/§7 read contract end to end: argv is `<op> read`,
    // and the wire carries the named ball as `metadata.bl-id` — then prints the
    // one line the fold captures.
    bind_script(
        tmp.path(),
        &landing,
        "fake",
        "#!/bin/sh\npayload=$(cat)\ncase \"$payload\" in *'\"bl-id\":[\"bl-7\"]'*) ;; *) exit 1;; esac\necho \"line $1 $2 $BALLS_PLUGIN_NAME\"\n",
    );
    assert_eq!(fold(&edge, tmp.path(), Verb::Show, "bl-7"), "line show read fake\n");
}

#[test]
fn fold_skips_a_failing_plugin_but_keeps_the_others_lines() {
    // Non-fatal by §6: a non-zero exit drops THAT plugin's contribution; the
    // chain's other lines still fold, in list order.
    let tmp = TempDir::new().unwrap();
    let edge = edge(tmp.path(), 0);
    let landing = landing_with(&edge, "[hooks]\n\"show\" = [\"bad\", \"good\"]\n");
    bind_script(tmp.path(), &landing, "bad", "#!/bin/sh\ncat > /dev/null\necho noise\nexit 3\n");
    bind_script(tmp.path(), &landing, "good", "#!/bin/sh\ncat > /dev/null\necho kept\n");
    assert_eq!(fold(&edge, tmp.path(), Verb::Show, "bl-1"), "kept\n");
}

#[test]
fn fold_is_empty_at_the_recursion_cap() {
    // At the §6 depth cap no further plugin may spawn — the read renders bare.
    let tmp = TempDir::new().unwrap();
    assert_eq!(fold(&edge(tmp.path(), DEPTH_CAP), tmp.path(), Verb::Show, "bl-1"), "");
}

#[test]
fn fold_is_empty_when_nothing_is_wired_or_the_schedule_is_malformed() {
    // No landing at all → an empty schedule → nothing to run.
    let tmp = TempDir::new().unwrap();
    assert_eq!(fold(&edge(tmp.path(), 0), tmp.path(), Verb::Show, "bl-1"), "");
    // A malformed plugins.toml is non-fatal on a read: fold contributes nothing.
    let broken = TempDir::new().unwrap();
    let e = edge(broken.path(), 0);
    landing_with(&e, "[hooks");
    assert_eq!(fold(&e, broken.path(), Verb::Show, "bl-1"), "");
}

#[test]
fn fold_is_empty_when_the_effective_config_is_malformed() {
    let tmp = TempDir::new().unwrap();
    let e = edge(tmp.path(), 0);
    let landing = landing_with(&e, SHOW_WIRED);
    bind_script(tmp.path(), &landing, "fake", "#!/bin/sh\necho never\n");
    fs::write(landing.join("config").join("balls.toml"), "not = toml = at all").unwrap();
    assert_eq!(fold(&e, tmp.path(), Verb::Show, "bl-1"), "");
}

#[test]
fn fold_skips_a_dangling_binding_and_an_unspawnable_binary() {
    // Wired but not bound (`bin/<name>` absent) — skipped, not an abort…
    let tmp = TempDir::new().unwrap();
    let e = edge(tmp.path(), 0);
    landing_with(&e, SHOW_WIRED);
    assert_eq!(fold(&e, tmp.path(), Verb::Show, "bl-1"), "");
    // …and a bound but non-executable file fails to spawn — also skipped.
    let tmp2 = TempDir::new().unwrap();
    let e2 = edge(tmp2.path(), 0);
    let landing2 = landing_with(&e2, SHOW_WIRED);
    let bin = tmp2.path().join("fake");
    fs::write(&bin, "not executable").unwrap();
    Registry::at(&landing2).bind("fake", &bin).unwrap();
    assert_eq!(fold(&e2, tmp2.path(), Verb::Show, "bl-1"), "");
}
