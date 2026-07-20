//! End-to-end cover of the `bl list` query surface (§9) — the single listing
//! verb driven through the real binary against throwaway primed repos. Each test
//! builds its own live/dead fixture, then asserts the OBSERVABLE stdout: every
//! filter surfaces exactly the ids it should and excludes the rest.
//!
//! The read view is glyph-free `--plain` here regardless of the flag: assert_cmd
//! pipes stdout (non-tty), so the badge is always the padded status word
//! (`ready`/`blocked`/`claimed`/`closed`, [`src/reads/style.rs`]). We lean on
//! that stable text.

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::{contains, is_match};
use std::path::Path;
use tempfile::TempDir;

/// `bl` rooted in `project`, with `HOME`/`$XDG_STATE_HOME` pinned under the
/// tempdir so the clone bundle never touches the real `$HOME`; `XDG_CONFIG_HOME`
/// dropped so no host config (workhours clock, remotes) leaks in. Plugin-depth
/// env is scrubbed so a `bl`-in-hook parent can't misroute the child dispatch.
fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("BALLS_PLUGIN_DEPTH")
        .env_remove("BALLS_PLUGIN_NAME");
    cmd
}

/// `git -C <cwd> <args>`, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

/// A real project repo on `main` with a seed commit, plus a primed checkout —
/// so the delivery plugin can fork `work/<id>` worktrees.
fn primed(tmp: &Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let (home, state, project) = (tmp.join("h"), tmp.join("s"), tmp.join("p"));
    std::fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    git(&project, &["config", "user.name", "test"]);
    git(&project, &["config", "user.email", "test@example.com"]);
    std::fs::write(project.join("seed.txt"), "x").unwrap();
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "seed"]);
    bl(&project, &home, &state).arg("prime").assert().success();
    (project, home, state)
}

/// A verb's one trimmed stdout product (create's id).
fn out(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// The `created` unix second of the first row of `bl list --json`.
fn first_created(project: &Path, home: &Path, state: &Path) -> i64 {
    let json = out(bl(project, home, state).args(["list", "--json"]).assert().success());
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    v[0]["created"].as_i64().unwrap()
}

/// The `YYYY-MM-DD` UTC calendar day of `ts + delta_days`, matching
/// `civil::start_of_day`'s UTC parse — computed by shelling `date -u` so the
/// window bounds track the fixture's real clock, never a hardcoded today.
fn utc_day(ts: i64, delta_days: i64) -> String {
    let at = ts + delta_days * 86_400;
    let o = std::process::Command::new("date").args(["-u", "-d", &format!("@{at}"), "+%Y-%m-%d"]).output().unwrap();
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

#[test]
fn live_status_rungs_partition_and_carry_a_claim_age() {
    // §3 derives three live rungs on read: ready (claimable), blocked (an
    // unresolved claim-blocker), claimed (someone holds it). `-s RUNG` narrows to
    // exactly one; the partition is disjoint and covering.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed(tmp.path());

    let alpha = out(bl(&project, &home, &state)
        .args(["create", "Alpha", "-t", "foo", "-p", "5", "--as", "me"])
        .assert()
        .success());
    let beta =
        out(bl(&project, &home, &state).args(["create", "Beta", "--needs", &alpha, "--as", "me"]).assert().success());
    let gamma = out(bl(&project, &home, &state).args(["create", "Gamma", "--as", "me"]).assert().success());
    bl(&project, &home, &state).args(["claim", &gamma, "--as", "me"]).assert().success();

    // ready = alpha only (beta is blocked by alpha, gamma is held).
    bl(&project, &home, &state)
        .args(["list", "-s", "ready"])
        .assert()
        .success()
        .stdout(contains(&alpha).and(contains(&beta).not()).and(contains(&gamma).not()));
    // blocked = beta only.
    bl(&project, &home, &state)
        .args(["list", "-s", "blocked"])
        .assert()
        .success()
        .stdout(contains(&beta).and(contains(&alpha).not()).and(contains(&gamma).not()));
    // claimed = gamma only, and its `@me` occupancy hangs a derived claim-age
    // suffix ` (<n>m)` — human-only, computed from the claim commit (bl-46ef).
    bl(&project, &home, &state)
        .args(["list", "-s", "claimed"])
        .assert()
        .success()
        .stdout(
            contains(&gamma)
                .and(contains(&alpha).not())
                .and(contains(&beta).not())
                .and(is_match(r"@me \(\d+m\)").unwrap()),
        );

    // Explicit `--plain`: the badge is the padded status WORD, not a glyph.
    bl(&project, &home, &state)
        .args(["list", "--plain"])
        .assert()
        .success()
        .stdout(contains("ready ").and(contains("blocked ")).and(contains("claimed ")));
    // Default (unfiltered) covers all three live ids.
    bl(&project, &home, &state)
        .args(["list"])
        .assert()
        .success()
        .stdout(contains(&alpha).and(contains(&beta)).and(contains(&gamma)));
}

#[test]
fn tag_needle_and_claimant_compose_as_and() {
    // The compose-AND filters (§9): every active predicate must hold. `--tag`
    // repeats AND-subset; NEEDLE is a title/body substring; `--claimant` is exact.
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed(tmp.path());

    let t1 = out(bl(&p, &h, &s).args(["create", "Widget", "-t", "foo", "-t", "bar", "--as", "me"]).assert().success());
    let t2 = out(bl(&p, &h, &s).args(["create", "Gadget", "-t", "foo", "--as", "me"]).assert().success());
    let t3 = out(bl(&p, &h, &s).args(["create", "Zebra unique", "--as", "me"]).assert().success());

    // `--tag foo` = the foo-tagged pair; T3 (untagged) is out.
    bl(&p, &h, &s)
        .args(["list", "--tag", "foo"])
        .assert()
        .success()
        .stdout(contains(&t1).and(contains(&t2)).and(contains(&t3).not()));
    // `--tag foo --tag bar` = AND-composed → only the dual-tagged T1.
    bl(&p, &h, &s)
        .args(["list", "--tag", "foo", "--tag", "bar"])
        .assert()
        .success()
        .stdout(contains(&t1).and(contains(&t2).not()).and(contains(&t3).not()));
    // NEEDLE = case-insensitive title substring → T3 alone.
    bl(&p, &h, &s)
        .args(["list", "zebra"])
        .assert()
        .success()
        .stdout(contains(&t3).and(contains(&t1).not()).and(contains(&t2).not()));

    // `--claimant me` (live): a bare `--as me` create does NOT claim, so nothing
    // matches until a real claim sets the field — then only that ball surfaces.
    bl(&p, &h, &s).args(["list", "--claimant", "me"]).assert().success().stdout(contains(&t1).not());
    bl(&p, &h, &s).args(["claim", &t1, "--as", "me"]).assert().success();
    bl(&p, &h, &s)
        .args(["list", "--claimant", "me"])
        .assert()
        .success()
        .stdout(contains(&t1).and(contains(&t2).not()).and(contains(&t3).not()));
}

#[test]
fn all_and_closed_reconstruct_a_dead_row_with_its_claimant() {
    // `-s closed`/`--all` reach the dead set from history (§9): a closed ball has
    // no file, so its row is reconstructed from the archive commit, badged
    // `closed`, retaining its stored `claimant` for `-s closed --claimant`.
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed(tmp.path());

    let dead = out(bl(&p, &h, &s).args(["create", "Doomed", "--as", "me"]).assert().success());
    let live = out(bl(&p, &h, &s).args(["create", "Survivor", "--as", "me"]).assert().success());
    bl(&p, &h, &s).args(["claim", &dead, "--as", "me"]).assert().success();
    bl(&p, &h, &s).args(["close", &dead, "--as", "me"]).assert().success();

    // Default (live) excludes the closed ball, includes the survivor.
    bl(&p, &h, &s).args(["list"]).assert().success().stdout(contains(&live).and(contains(&dead).not()));
    // `-s closed` = the dead set alone, badged `closed`.
    bl(&p, &h, &s)
        .args(["list", "-s", "closed"])
        .assert()
        .success()
        .stdout(contains(&dead).and(contains("closed ")).and(contains(&live).not()));
    // `--all` = live + dead together.
    bl(&p, &h, &s).args(["list", "--all"]).assert().success().stdout(contains(&dead).and(contains(&live)));
    // `-s closed --claimant me` answers "what did me deliver" from the archive.
    bl(&p, &h, &s)
        .args(["list", "-s", "closed", "--claimant", "me"])
        .assert()
        .success()
        .stdout(contains(&dead));
    // A non-matching claimant on the dead set yields nothing.
    bl(&p, &h, &s)
        .args(["list", "-s", "closed", "--claimant", "ghost"])
        .assert()
        .success()
        .stdout(contains(&dead).not());
}

#[test]
fn date_windows_bound_the_set_and_legacy_rejects_a_dead_reach() {
    // `--since`/`--until` are UTC calendar bounds over created-OR-updated (§9);
    // `--legacy` serves the LIVE legacy set alone, so pairing it with a dead-set
    // reach (`--all`) is a usage contradiction caught at parse.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed(tmp.path());

    let one = out(bl(&project, &home, &state).args(["create", "One", "--as", "me"]).assert().success());
    let two = out(bl(&project, &home, &state).args(["create", "Two", "--as", "me"]).assert().success());
    let ts = first_created(&project, &home, &state);
    let (today, yest, tom) = (utc_day(ts, 0), utc_day(ts, -1), utc_day(ts, 1));

    // A window spanning today catches both live balls.
    bl(&project, &home, &state)
        .args(["list", "--since", &today, "--until", &today])
        .assert()
        .success()
        .stdout(contains(&one).and(contains(&two)));
    // `--until yesterday` closes before either was born → empty.
    bl(&project, &home, &state)
        .args(["list", "--until", &yest])
        .assert()
        .success()
        .stdout(contains(&one).not().and(contains(&two).not()));
    // `--since tomorrow` opens after → empty.
    bl(&project, &home, &state)
        .args(["list", "--since", &tom])
        .assert()
        .success()
        .stdout(contains(&one).not().and(contains(&two).not()));

    // `--legacy` + `--all`: the live-only legacy preview can't take a dead reach.
    bl(&project, &home, &state)
        .args(["list", "--legacy", "--all"])
        .assert()
        .failure()
        .stderr(contains("--legacy serves the live legacy set").and(contains("no --all/--status closed reach")));
}
