//! The machine `--json` CONTRACTS agents script against (§9), driven through the
//! real binary against throwaway primed repos. `list --json`/`show --json` are
//! the bedrock: the lossless stored-frontmatter mirror ([`src/reads/record.rs`]),
//! NOTHING derived. Each test pins a promise the human render is free to break:
//! no claim-age key, a faithful dead-row reconstruction, the retirement-date
//! window, the compose-AND "my own claims" query, and the §10 priority order.
//!
//! Op instants are pinned with `BALLS_CLOCK` (the i64 test seam, [`src/clock.rs`])
//! so `created`/`retired_at` land on distinct days — deterministic date windows
//! and priority tiebreaks, never a same-second race.

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

/// `bl` rooted in `project`, host config scrubbed so no workhours clock/remote
/// leaks in, plugin-depth env dropped so a hook parent can't misroute dispatch.
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
/// so the delivery plugin can fork `work/<id>` worktrees on claim/close.
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

/// A verb's one trimmed stdout product (create's id, or a `--json` blob).
fn out(a: Assert) -> String {
    String::from_utf8(a.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// The `YYYY-MM-DD` UTC calendar day of unix `ts`, matching the window parser's
/// UTC parse — shelled `date -u` so bounds track the pinned clock, not a today.
fn utc_day(ts: i64) -> String {
    let o = std::process::Command::new("date").args(["-u", "-d", &format!("@{ts}"), "+%Y-%m-%d"]).output().unwrap();
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

/// The `id`s of `bl list --json`, in the emitted array order (the §10 order).
fn json_ids(a: Assert) -> Vec<String> {
    let arr: Vec<serde_json::Value> = serde_json::from_str(&out(a)).unwrap();
    arr.iter().map(|r| r["id"].as_str().unwrap().to_string()).collect()
}

/// The one row of a `--json` array whose `id` equals `id`, parsed.
fn row(json: &str, id: &str) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = serde_json::from_str(json).unwrap();
    arr.into_iter().find(|r| r["id"].as_str() == Some(id)).unwrap_or_else(|| panic!("no row for {id} in:\n{json}"))
}

#[test]
fn a_claimed_json_row_carries_stored_frontmatter_only_no_derived_claim_age() {
    // The human `-s claimed` view hangs a DERIVED ` (Nm)` claim-age off `@me`
    // (skill/list.md), but `--json` is the bedrock stored mirror: the record has
    // `claimant`, and NOTHING age-derived (no `claim_age`/`claimed_at`/`age` key).
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed(tmp.path());
    let id = out(bl(&p, &h, &s).args(["create", "Held", "--as", "me"]).assert().success());
    bl(&p, &h, &s).args(["claim", &id, "--as", "me"]).assert().success();

    let json = out(bl(&p, &h, &s).args(["list", "-s", "claimed", "--json"]).assert().success());
    let r = row(&json, &id);
    assert_eq!(r["claimant"].as_str(), Some("me"), "the stored claimant rides the record:\n{json}");
    let obj = r.as_object().unwrap();
    assert!(obj.get("claim_age").is_none() && obj.get("claimed_at").is_none() && obj.get("age").is_none());
    for k in obj.keys() {
        assert!(!k.contains("age") && !k.contains("claimed_at"), "no derived claim-age key on the row: {k}");
    }
}

#[test]
fn a_closed_json_row_round_trips_id_claimant_tags_and_timestamps() {
    // `-s closed`/`--all` reconstruct the dead row from the archive commit (§9);
    // its `--json` is still the bedrock record, so id/claimant/tags/created all
    // round-trip the ball as it stood the instant before deletion.
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed(tmp.path());
    let id = out(bl(&p, &h, &s).args(["create", "Doomed", "-t", "foo", "-t", "bar", "--as", "me"]).assert().success());
    let live = out(bl(&p, &h, &s).args(["show", &id, "--json"]).assert().success());
    let created = serde_json::from_str::<serde_json::Value>(&live).unwrap()["created"].as_i64().unwrap();
    bl(&p, &h, &s).args(["claim", &id, "--as", "me"]).assert().success();
    bl(&p, &h, &s).args(["close", &id, "--as", "me"]).assert().success();

    let reaches: [&[&str]; 2] = [&["list", "-s", "closed", "--json"], &["list", "--all", "--json"]];
    for reach in reaches {
        let json = out(bl(&p, &h, &s).args(reach).assert().success());
        let r = row(&json, &id);
        assert_eq!(r["id"].as_str(), Some(id.as_str()), "id round-trips ({reach:?}):\n{json}");
        assert_eq!(r["claimant"].as_str(), Some("me"), "stored claimant survives archival ({reach:?})");
        let tags: Vec<&str> = r["tags"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
        assert_eq!(tags, vec!["foo", "bar"], "tags round-trip ({reach:?})");
        assert_eq!(r["created"].as_i64(), Some(created), "created ts is the literal stored i64 ({reach:?})");
        assert!(r["updated"].as_i64().is_some(), "updated is an integer timestamp ({reach:?})");
    }
}

#[test]
fn a_dead_row_windows_on_its_retirement_date_not_its_stored_updated() {
    // A dead row's date filter reads `retired_at` (the deletion-commit date), a
    // DISTINCT path from a live row's stored `updated` (src/reads/list.rs:80,
    // filter.rs). Pin create/claim/close on three different days via BALLS_CLOCK,
    // then window each edge.
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed(tmp.path());
    let (t_create, t_claim, t_close) = (1_600_000_000_i64, 1_620_000_000_i64, 1_640_000_000_i64);
    let clocked = |t: i64, args: &[&str]| bl(&p, &h, &s).env("BALLS_CLOCK", t.to_string()).args(args).assert().success();
    let id = out(clocked(t_create, &["create", "Doomed", "--as", "me"]));
    clocked(t_claim, &["claim", &id, "--as", "me"]);
    clocked(t_close, &["close", &id, "--as", "me"]);

    let win = |from: i64, to: i64| {
        bl(&p, &h, &s).args(["list", "-s", "closed", "--since", &utc_day(from), "--until", &utc_day(to)]).assert()
    };
    // The RETIREMENT-day window catches the dead row — retired_at feeds the filter.
    win(t_close, t_close).success().stdout(contains(&id));
    // The CLAIM-day window (its stored `updated`) does NOT — the dead path ignores
    // `updated`, so t_claim never matches; only created/retired_at can.
    win(t_claim, t_claim).success().stdout(contains(&id).not());
    // The CREATE-day window catches it — `created` is checked too (created OR date).
    win(t_create, t_create).success().stdout(contains(&id));
}

#[test]
fn s_claimed_and_claimant_compose_to_my_own_claims() {
    // The "find my own claims" query: `-s claimed` (the live held rung) AND
    // `--claimant me` compose so only balls I hold surface — a peer's held ball
    // is out, and the symmetric `--claimant alice` sees only theirs.
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed(tmp.path());
    let mine = out(bl(&p, &h, &s).args(["create", "Mine", "--as", "me"]).assert().success());
    let theirs = out(bl(&p, &h, &s).args(["create", "Theirs", "--as", "me"]).assert().success());
    bl(&p, &h, &s).args(["claim", &mine, "--as", "me"]).assert().success();
    bl(&p, &h, &s).args(["claim", &theirs, "--as", "alice"]).assert().success();

    bl(&p, &h, &s)
        .args(["list", "-s", "claimed", "--claimant", "me"])
        .assert()
        .success()
        .stdout(contains(&mine).and(contains(&theirs).not()));
    bl(&p, &h, &s)
        .args(["list", "-s", "claimed", "--claimant", "alice"])
        .assert()
        .success()
        .stdout(contains(&theirs).and(contains(&mine).not()));
}

#[test]
fn json_orders_by_priority_ascending_absent_last_and_reshuffles_on_update() {
    // §10 order (src/reads/list.rs `order_key`): present priorities ASCENDING
    // (1,2,3 — skill/list.md "highest priority first", p1 = highest), no-priority
    // LAST, ties broken by `created`. Create in MIXED order on rising clocks so it
    // is the priority — not creation order — that sorts.
    let tmp = TempDir::new().unwrap();
    let (p, h, s) = primed(tmp.path());
    let base = 1_600_000_000_i64;
    let mk = |sec: i64, title: &str, args: &[&str]| {
        let mut v = vec!["create", title];
        v.extend_from_slice(args);
        v.extend_from_slice(&["--as", "me"]);
        out(bl(&p, &h, &s).env("BALLS_CLOCK", (base + sec).to_string()).args(v).assert().success())
    };
    let three = mk(0, "Three", &["-p", "3"]);
    let one = mk(10, "One", &["-p", "1"]);
    let none = mk(20, "None", &[]);
    let two = mk(30, "Two", &["-p", "2"]);

    // ascending priority, absent last: [1, 2, 3, none].
    let ids = json_ids(bl(&p, &h, &s).args(["list", "--json"]).assert().success());
    assert_eq!(ids, vec![one.clone(), two.clone(), three.clone(), none.clone()], "priority-ordered, absent last");

    // Reshuffle: `none` → p1 (now ties `one`; `one` was created first, so leads),
    // `three` → no priority (drops to last). Order becomes [one, none, two, three].
    bl(&p, &h, &s).args(["update", &none, "-p", "1", "--as", "me"]).assert().success();
    bl(&p, &h, &s).args(["update", &three, "--no-priority", "--as", "me"]).assert().success();
    let ids2 = json_ids(bl(&p, &h, &s).args(["list", "--json"]).assert().success());
    assert_eq!(ids2, vec![one, none, two, three], "re-priority reshuffles by the same key");
}
