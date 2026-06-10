//! Tests for `bl conf` writes (bl-c2de) — each key's canonical home, the
//! landing seal with its no-change convergence, the XDG file edit, and the §4
//! list compose applied at write time.

use crate::conf;
use crate::edge::Edge;
use crate::git;
use crate::layout::{CloneDir, Xdg};
use crate::substrate;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn edge(tmp: &TempDir) -> Edge {
    Edge {
        xdg: Xdg::with(tmp.path(), None, Some(&tmp.path().join("state").to_string_lossy())),
        invocation_path: tmp.path().join("proj"),
        default_actor: "tester".into(),
        depth: 0,
        exe_dir: None,
        color: false,
        log_level: None,
    }
}

fn founded(e: &Edge) -> CloneDir {
    let clone = e.xdg.clone_dir(&e.invocation_path);
    substrate::found(&clone.landing(), &clone.store(), &e.xdg, None).unwrap();
    clone
}

/// Run `bl conf <argv>` through the verb's own dispatch.
fn conf(e: &Edge, argv: &[&str]) -> std::io::Result<()> {
    conf::run(e, &argv.iter().map(ToString::to_string).collect::<Vec<_>>())
}

fn commits(landing: &Path) -> usize {
    git::run(landing, &["rev-list", "--count", "HEAD"], None).unwrap().trim().parse().unwrap()
}

fn subject(landing: &Path) -> String {
    git::run(landing, &["log", "-1", "--format=%s"], None).unwrap().trim().to_string()
}

fn landing_text(clone: &CloneDir, name: &str) -> String {
    fs::read_to_string(clone.landing().join("config").join(name)).unwrap()
}

#[test]
fn set_task_branch_seals_one_commit_and_converges_on_a_repeat() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    let before = commits(&clone.landing());
    conf(&e, &["set", "task-branch", "balls/x"]).unwrap();
    assert!(landing_text(&clone, "balls.toml").contains("tasks_branch = \"balls/x\""));
    assert_eq!(commits(&clone.landing()), before + 1);
    assert_eq!(subject(&clone.landing()), "balls: conf set task-branch balls/x");
    // The same value again is the §13 no-op converge — nothing new seals.
    conf(&e, &["set", "task-branch", "balls/x"]).unwrap();
    assert_eq!(commits(&clone.landing()), before + 1);
}

#[test]
fn set_task_branch_to_the_landing_is_refused_and_writes_nothing() {
    // §2/§4 (bl-ac89): the coincident name is refused at the front door — same
    // write-time validation precedent as log-level's ladder check. Nothing is
    // written and nothing seals, so the checkout is never poisoned.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    let before = commits(&clone.landing());
    let err = conf(&e, &["set", "task-branch", "balls/config"]).unwrap_err().to_string();
    assert!(err.contains("names the landing"), "{err}");
    assert_eq!(commits(&clone.landing()), before);
}

#[test]
fn a_conf_seal_carries_the_checkout_scoped_trailers() {
    // §5: checkout-scoped seals carry bl-protocol/bl-op/bl-actor — only bl-id
    // (which names a single ball) is absent (bl-1d9b).
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    conf(&e, &["set", "task-branch", "balls/x"]).unwrap();
    let msg = git::run(&clone.landing(), &["log", "-1", "--format=%B"], None).unwrap();
    let md = crate::message::parse(&msg).unwrap();
    assert_eq!(md["bl-protocol"], ["1"], "{msg}");
    assert_eq!(md["bl-op"], ["conf"], "{msg}");
    assert_eq!(md["bl-actor"], ["tester"], "{msg}");
    assert!(!md.contains_key("bl-id"), "{msg}");
}

#[test]
fn set_log_level_validates_against_the_ladder() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    assert!(conf(&e, &["set", "log-level", "noisy"]).is_err()); // not a level
    conf(&e, &["set", "log-level", "debug"]).unwrap();
    assert!(landing_text(&clone, "balls.toml").contains("log_level = \"debug\""));
}

#[test]
fn set_task_remote_writes_the_xdg_file_preserving_other_keys() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    founded(&e);
    // Works with no user config yet (dirs + file created)…
    conf(&e, &["set", "task-remote", "git@hub:r"]).unwrap();
    let body = fs::read_to_string(e.xdg.user_config()).unwrap();
    assert!(body.contains("remote = \"git@hub:r\""), "{body}");
    // …and edits ONE key when one exists, the rest round-tripping.
    fs::write(e.xdg.user_config(), "log_level = \"debug\"\nremote = \"old\"\n").unwrap();
    conf(&e, &["set", "task-remote", "git@hub:new"]).unwrap();
    let body = fs::read_to_string(e.xdg.user_config()).unwrap();
    assert!(body.contains("remote = \"git@hub:new\"") && body.contains("log_level = \"debug\""), "{body}");
}

#[test]
fn hook_compose_applies_the_directive_and_converges() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    conf(&e, &["append", "close.pre", "x"]).unwrap();
    assert!(landing_text(&clone, "plugins.toml").contains("\"close.pre\" = [\"x\"]"));
    let sealed = commits(&clone.landing());
    // A present name re-appended (or re-prepended) is the convergent no-op.
    conf(&e, &["append", "close.pre", "x"]).unwrap();
    conf(&e, &["prepend", "close.pre", "x"]).unwrap();
    assert_eq!(commits(&clone.landing()), sealed);
    conf(&e, &["prepend", "close.pre", "y"]).unwrap();
    assert!(landing_text(&clone, "plugins.toml").contains("\"close.pre\" = [\"y\", \"x\"]"));
    // Removing an absent name converges too; removing the last drops the key.
    conf(&e, &["remove", "close.pre", "ghost"]).unwrap();
    conf(&e, &["remove", "close.pre", "y"]).unwrap();
    conf(&e, &["remove", "close.pre", "x"]).unwrap();
    assert!(!landing_text(&clone, "plugins.toml").contains("close.pre"));
}

#[test]
fn set_on_a_hooks_key_bare_replaces_the_whole_list() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    conf(&e, &["set", "close.pre", "a", "b"]).unwrap();
    assert!(landing_text(&clone, "plugins.toml").contains("\"close.pre\" = [\"a\", \"b\"]"));
    // Replacing with nothing empties the list — the key drops (§4 absent/empty).
    conf(&e, &["set", "close.pre"]).unwrap();
    assert!(!landing_text(&clone, "plugins.toml").contains("close.pre"));
}

#[test]
fn an_empty_plugin_name_is_refused_and_writes_nothing() {
    // bl-bee0: `set <hooks-key> ""` wrote [""], which dispatch later resolved
    // to bin/ itself (EACCES). A plugin name is non-empty — clearing the list
    // already has its spelling (`set <key>` with no values). Refused at the
    // front door (the bl-ac89 precedent), nothing written, nothing sealed.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    let before = commits(&clone.landing());
    for argv in [
        vec!["set", "close.pre", ""],
        vec!["set", "close.pre", "a", ""],
        vec!["append", "close.pre", ""],
        vec!["prepend", "close.pre", ""],
        vec!["remove", "close.pre", ""],
    ] {
        let err = conf(&e, &argv).unwrap_err().to_string();
        assert!(err.contains("non-empty"), "{argv:?}: {err}");
    }
    assert!(!landing_text(&clone, "plugins.toml").contains("close.pre"));
    assert_eq!(commits(&clone.landing()), before);
}

#[test]
fn foreign_tables_round_trip_untouched() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    fs::write(
        clone.landing().join("config").join("plugins.toml"),
        "[team]\nkeep = \"this\"\n\n[hooks]\n\"close.pre\" = [\"a\"]\n",
    )
    .unwrap();
    conf(&e, &["append", "close.pre", "b"]).unwrap();
    let body = landing_text(&clone, "plugins.toml");
    assert!(body.contains("keep = \"this\""), "{body}");
    assert!(body.contains("\"close.pre\" = [\"a\", \"b\"]"), "{body}");
}

#[test]
fn a_non_table_hooks_root_is_refused() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    fs::write(clone.landing().join("config").join("plugins.toml"), "hooks = \"nope\"\n").unwrap();
    let err = conf(&e, &["append", "close.pre", "x"]).unwrap_err().to_string();
    assert!(err.contains("[hooks] is not a table"), "{err}");
}

#[test]
fn the_write_grammar_is_enforced() {
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    founded(&e);
    for (argv, expect) in [
        (vec!["set"], "needs <key>"),
        (vec!["set", "bogus", "v"], "unknown key"),
        (vec!["set", "task-branch", "a", "b"], "exactly one value"),
        (vec!["set", "task-remote"], "exactly one value"),
        (vec!["append", "task-branch", "x"], "is a scalar"),
        (vec!["remove", "log-level", "x"], "is a scalar"),
        (vec!["append", "close.pre"], "exactly one value"),
        (vec!["append", "close.pre", "a", "b"], "exactly one value"),
    ] {
        let err = conf(&e, &argv).unwrap_err().to_string();
        assert!(err.contains(expect), "{argv:?}: {err}");
    }
}
