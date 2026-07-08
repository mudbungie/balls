//! Tests for `bl conf` writes (bl-c2de) — each key's canonical home, the
//! landing seal with its no-change convergence, the per-clone binding edit
//! (bl-d081), and the §4 list compose applied at write time.

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
        path_dirs: Vec::new(),
        color: false,
        log_level: None,
        balls_clock: None,
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
fn set_task_remote_writes_the_per_clone_binding_preserving_other_keys() {
    // bl-d081: a URL's durable home is THIS clone's binding.toml (per-checkout
    // local state), not the machine-wide XDG file that silently shadowed every
    // other repo's store.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    // Works with no binding file yet (file created)…
    conf(&e, &["set", "task-remote", "git@hub:r"]).unwrap();
    let body = fs::read_to_string(clone.binding()).unwrap();
    assert!(body.contains("remote = \"git@hub:r\""), "{body}");
    // …and edits ONE key when one exists, the rest round-tripping.
    fs::write(clone.binding(), "tasks_branch = \"balls/x\"\nremote = \"old\"\n").unwrap();
    conf(&e, &["set", "task-remote", "git@hub:new"]).unwrap();
    let body = fs::read_to_string(clone.binding()).unwrap();
    assert!(body.contains("remote = \"git@hub:new\"") && body.contains("tasks_branch = \"balls/x\""), "{body}");
    // The machine-wide XDG file is NEVER touched — no cross-repo shadowing.
    assert!(!e.xdg.user_config().exists(), "the global XDG config must not be written");
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

#[test]
fn set_task_remote_sentinel_is_a_landing_policy_write_a_url_clears_it() {
    // bl-9df0: the key's home routes by VALUE. The stealth sentinel is
    // per-checkout policy → a sealed landing edit (what `prime --stealth`
    // sugars to); a URL is per-clone → this clone's binding.toml (bl-d081), AND
    // the sentinel is cleared so the set changes what the ladder actually
    // resolves (the landing rung outranks binding — leaving it would be the
    // bl-d234 trap).
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp);
    let clone = founded(&e);
    conf(&e, &["set", "task-remote", "none"]).unwrap();
    assert!(landing_text(&clone, "balls.toml").contains("task_remote = \"none\""));
    assert_eq!(subject(&clone.landing()), "balls: conf set task-remote none");
    conf(&e, &["set", "task-remote", "git@hub:r"]).unwrap();
    assert!(!landing_text(&clone, "balls.toml").contains("task_remote"));
    assert_eq!(subject(&clone.landing()), "balls: conf set task-remote git@hub:r");
    let binding = fs::read_to_string(clone.binding()).unwrap();
    assert!(binding.contains("remote = \"git@hub:r\""), "{binding}");
}

// The `[hooks]` list-compose writes share this module's edge/founded/conf fixtures.
#[path = "conf_write_hooks_tests.rs"]
mod hooks;
