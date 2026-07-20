//! The WRITE half of `bl conf` (bl-d2d8): scope-keyed CRUD where the KEY implies
//! its canonical file — the store `remote` + `clock_provider` on this clone's
//! local-state `binding.toml`, the stealth sentinel + `task-branch`/`log-level`
//! on the landing `balls.toml`, the `[hooks]` schedule on `plugins.toml`. Each
//! set is verified where it lands; each refusal exits nonzero with its message.

use predicates::str::contains;
use tempfile::TempDir;

use crate::{balls_toml, binding_toml, bl, landing, plugins_toml, primed_project, read_is};

/// `set task-remote <url>` binds this clone AND clears a prior stealth sentinel,
/// so what the ladder resolves actually changes; `set task-remote none` declares
/// stealth on the landing. The two spellings land in two different files.
#[test]
fn set_url_binds_the_clone_and_clears_the_stealth_sentinel() {
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed_project(tmp.path());

    // Declare stealth: the sentinel lands (committed) in the landing balls.toml.
    bl(&p, &h, &s).args(["conf", "set", "task-remote", "none"]).assert().success().stderr(contains("conf set task-remote"));
    let land = landing(&p, &h, &s);
    assert!(balls_toml(&land).contains("task_remote = \"none\""), "sentinel: {}", balls_toml(&land));

    // Set a URL: it writes the local-state binding.toml AND clears the landing
    // sentinel — leaving it would make the set change nothing the ladder reads.
    bl(&p, &h, &s).args(["conf", "set", "task-remote", "git@host:r.git"]).assert().success();
    assert!(binding_toml(&land).contains("remote = \"git@host:r.git\""), "binding: {}", binding_toml(&land));
    assert!(!balls_toml(&land).contains("task_remote"), "sentinel not cleared: {}", balls_toml(&land));
    read_is(&p, &h, &s, "task-remote", "git@host:r.git", "binding");
}

/// `set task-branch` seals the landing balls.toml; the coincident landing-branch
/// name is refused at the front door (bl-ac89) — one branch can't back two
/// checkouts.
#[test]
fn set_task_branch_lands_and_forbids_the_landing_name() {
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed_project(tmp.path());

    bl(&p, &h, &s).args(["conf", "set", "task-branch", "feature"]).assert().success().stderr(contains("conf set task-branch"));
    let land = landing(&p, &h, &s);
    assert!(balls_toml(&land).contains("tasks_branch = \"feature\""), "balls.toml: {}", balls_toml(&land));
    read_is(&p, &h, &s, "task-branch", "feature", "landing");

    bl(&p, &h, &s).args(["conf", "set", "task-branch", "balls/config"]).assert().failure().stderr(contains("names the landing"));
}

/// `set log-level` seals the landing balls.toml, but only after the level parses
/// — a bogus level is refused before anything is written.
#[test]
fn set_log_level_lands_and_refuses_an_unknown_level() {
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed_project(tmp.path());

    bl(&p, &h, &s).args(["conf", "set", "log-level", "error"]).assert().success().stderr(contains("conf set log-level"));
    let land = landing(&p, &h, &s);
    assert!(balls_toml(&land).contains("log_level = \"error\""), "balls.toml: {}", balls_toml(&land));
    read_is(&p, &h, &s, "log-level", "error", "landing");

    bl(&p, &h, &s).args(["conf", "set", "log-level", "nonsense"]).assert().failure().stderr(contains("unrecognised log level"));
}

/// `set clock-provider` writes THIS clone's local-state binding.toml, never a
/// landing field — it is box-local and must not travel on `install` (§8).
#[test]
fn set_clock_provider_writes_the_binding_not_the_landing() {
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed_project(tmp.path());

    bl(&p, &h, &s).args(["conf", "set", "clock-provider", "/usr/bin/date"]).assert().success().stderr(contains("conf set clock-provider"));
    let land = landing(&p, &h, &s);
    assert!(binding_toml(&land).contains("clock_provider = \"/usr/bin/date\""), "binding: {}", binding_toml(&land));
    assert!(!balls_toml(&land).contains("clock_provider"), "clock leaked into landing: {}", balls_toml(&land));
    read_is(&p, &h, &s, "clock-provider", "/usr/bin/date", "binding");
}

/// A wholesale `set` writes a `[hooks]` list to plugins.toml; removing the names
/// one by one drops the key LITERALLY from the file when it empties (§4: an
/// absent/empty list runs nothing — don't store `[]`).
#[test]
fn hooks_set_then_remove_last_name_drops_the_key() {
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed_project(tmp.path());

    bl(&p, &h, &s).args(["conf", "set", "close.post", "bl-tracker", "bl-delivery"]).assert().success().stderr(contains("conf set close.post"));
    read_is(&p, &h, &s, "close.post", "bl-tracker, bl-delivery", "landing");
    let land = landing(&p, &h, &s);
    let wired = plugins_toml(&land);
    assert!(wired.contains("close.post") && wired.contains("bl-tracker") && wired.contains("bl-delivery"), "plugins.toml: {wired}");

    bl(&p, &h, &s).args(["conf", "remove", "close.post", "bl-tracker"]).assert().success();
    bl(&p, &h, &s).args(["conf", "remove", "close.post", "bl-delivery"]).assert().success();
    let emptied = plugins_toml(&land);
    assert!(!emptied.contains("close.post"), "emptied key must be gone from the file: {emptied}");
    read_is(&p, &h, &s, "close.post", "(none)", "default");
}

/// The usage-error family: a list op on a scalar, an unknown key, a value on a
/// read, a keyless/valueless set, and `--as` (which `conf` never honors —
/// config is checkout-local, authored by the default actor). Each exits nonzero.
#[test]
fn usage_errors_reject_the_malformed_family() {
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed_project(tmp.path());

    let cases: [(&[&str], &str); 6] = [
        (&["conf", "append", "task-remote", "x"], "is a scalar"),
        (&["conf", "no-such-key"], "unknown key"),
        (&["conf", "task-remote", "extra"], "takes no value on a read"),
        (&["conf", "set"], "needs <key>"),
        (&["conf", "set", "task-remote"], "takes exactly one value"),
        (&["conf", "--as", "alice"], "takes no value on a read"),
    ];
    for (args, msg) in cases {
        bl(&p, &h, &s).args(args).assert().failure().stderr(contains(msg));
    }
}
