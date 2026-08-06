//! End-to-end for §16 `bl import` — the write inverse of the bedrock read.
//!
//! Drives the freshly-built `bl` over throwaway repos in a `TempDir`, with
//! `HOME`/`$XDG_STATE_HOME` pinned inside it (and `$XDG_CONFIG_HOME` scrubbed)
//! so each store's clone bundle lands in the tempdir, never the real `$HOME`.
//! Three properties, all asserted on the OBSERVABLE surface (stdout/stderr/exit
//! + `show --json`), never internals:
//!   * a `show --json | import --as me` pipe into a SECOND store reproduces the
//!     ball byte-for-byte — id + timestamps verbatim, nothing minted or stamped;
//!   * a stream carrying an in-store collision AND an intra-stream duplicate is
//!     refused wholesale BEFORE any write, naming the offending id(s);
//!   * every id `create` mints matches the shipped `IdScheme` (`bl-` + four
//!     lower-hex), and successful creates are never duplicated.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// One isolated `bl` world: a project repo on `main` plus the `HOME`/state the
/// store lands under. Two of these = two independent stores in one tempdir.
struct Store {
    project: PathBuf,
    home: PathBuf,
    state: PathBuf,
}

/// Run `git -C <cwd> <args>`, asserting success.
fn git(cwd: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git").arg("-C").arg(cwd).args(args).status().unwrap().success();
    assert!(ok, "git {args:?} failed in {}", cwd.display());
}

impl Store {
    /// Found a real project repo on `main` (a seed commit lets the delivery
    /// plugin fork `work/<id>` worktrees) and prime its checkout.
    ///
    /// The seed content is the `tag`, so every store's repo gets a DISTINCT
    /// root commit — the same rule tests/fleet.rs states. A constant seed made
    /// the root commit a function of the wall-clock SECOND alone (identical
    /// tree, message and identity, only the timestamp varying), so two stores
    /// founded inside one second shared a root and two straddling a boundary
    /// did not — and the root-aware `bl list` scope (bl-0161) then flipped with
    /// it. That coincidence is what made this file flake under load (bl-36f1).
    fn found(tmp: &Path, tag: &str) -> Store {
        let s = Store {
            project: tmp.join(format!("{tag}-p")),
            home: tmp.join(format!("{tag}-h")),
            state: tmp.join(format!("{tag}-s")),
        };
        std::fs::create_dir_all(&s.project).unwrap();
        git(&s.project, &["init", "-q", "-b", "main"]);
        git(&s.project, &["config", "user.name", "test"]);
        git(&s.project, &["config", "user.email", "test@example.com"]);
        std::fs::write(s.project.join("seed.txt"), tag).unwrap();
        git(&s.project, &["add", "-A"]);
        git(&s.project, &["commit", "-qm", "seed"]);
        s.cmd().arg("prime").assert().success();
        s
    }

    /// A fresh `bl` invocation rooted in this store's project, fully isolated —
    /// pinned `HOME`/state, no host `$XDG_CONFIG_HOME`, and the inherited
    /// plugin-chain env scrubbed (this target may itself run inside a hook).
    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("bl").unwrap();
        c.current_dir(&self.project)
            .env("HOME", &self.home)
            .env("XDG_STATE_HOME", &self.state)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("BALLS_PLUGIN_DEPTH")
            .env_remove("BALLS_PLUGIN_NAME");
        c
    }

    /// This store's `show <id> --json` bytes — the lossless bedrock record.
    fn show_json(&self, id: &str) -> String {
        let out = self.cmd().args(["show", id, "--json"]).assert().success();
        String::from_utf8(out.get_output().stdout.clone()).unwrap()
    }
}

/// The id `bl create` printed alone to stdout (§9).
fn created_id(out: assert_cmd::assert::Assert) -> String {
    String::from_utf8(out.get_output().stdout.clone()).unwrap().trim().to_string()
}

/// A minimal fully-identified bedrock record (one JSON object) for `id`.
fn record(id: &str) -> String {
    format!(r#"{{"id":"{id}","title":"T {id}","created":5,"updated":9,"body":"kept\n"}}"#)
}

/// Whether `id` is exactly what the shipped `IdScheme` mints: `bl-` + four
/// lower-hex digits (prefix "bl-", length 4, alphabet 0-9a-f).
fn is_scheme_id(id: &str) -> bool {
    let Some(hex) = id.strip_prefix("bl-") else { return false };
    hex.len() == 4 && hex.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[test]
fn plain_import_round_trips_a_ball_byte_for_byte() {
    // `show --json` OUT of store A, piped `import --as me` INTO store B, then
    // `show --json` out of B reproduces the SAME bytes: id, timestamps, body all
    // verbatim — the reproduction `create` refuses (it mints + stamps), which is
    // exactly why import exists (§16).
    let tmp = TempDir::new().unwrap();
    let a = Store::found(tmp.path(), "a");
    let b = Store::found(tmp.path(), "b");

    let id = created_id(
        a.cmd().args(["create", "Round trip", "--body", "kept body\n", "--as", "alice"]).assert().success(),
    );
    let record = a.show_json(&id);

    // The pipe: B holds nothing yet, so the import lands one ball (stdout stays
    // silent per §9 — the caller supplied the id — the count rides stderr).
    // The record still carries store A's `root_commit`, so B's default
    // root-aware scope will hide it — and the import SAYS so (bl-d3fa): the
    // confirmation is decorated with one hint naming the fact and the lifted
    // read, so the empty `bl list` below never reads as a lost import.
    b.cmd()
        .args(["import", "--as", "me"])
        .write_stdin(record.clone())
        .assert()
        .success()
        .stdout("")
        .stderr(contains("import 1 ball"))
        .stderr(contains("1 of 1 rooted in another project"))
        .stderr(contains("bl list --everywhere"));

    // Byte-equivalent record on the far side: nothing was minted or restamped.
    assert_eq!(record, b.show_json(&id), "imported ball must round-trip byte-for-byte in show --json");
    // And the id survived verbatim rather than a fresh mint being substituted.
    assert!(record.contains(&format!("\"id\": \"{id}\"")), "the source id is preserved: {record}");
    // Nothing extra minted: B holds exactly the one imported id. `--everywhere`
    // is the honest reach for that count — byte-for-byte means the imported
    // record still carries store A's `root_commit`, so B's checkout is a
    // different project and the default root-aware scope (bl-0161) rightly
    // hides the row. Both halves are asserted: the scope hides it, the lifted
    // scope holds exactly one of it.
    b.cmd().args(["list", "--json"]).assert().success().stdout(contains(&id[..]).count(0));
    b.cmd().args(["list", "--everywhere", "--json"]).assert().success().stdout(contains(&id[..]).count(1));
}

#[test]
fn a_colliding_stream_is_refused_wholesale_before_any_write() {
    // Seed B with a held id, then hand import a stream that both re-imports that
    // id AND repeats a fresh one within the stream. Refuse-don't-guess (§16):
    // the WHOLE stream aborts before a byte is written, exit nonzero, naming
    // every offending id — so the genuinely-fresh record between them is dropped.
    let tmp = TempDir::new().unwrap();
    let b = Store::found(tmp.path(), "b");
    // The other half of bl-d3fa: `record()` carries no `root_commit`, so the
    // ball is admitted everywhere and the default scope shows it — no surprise,
    // therefore no hint. The line decorates the hidden case only.
    b.cmd()
        .args(["import", "--as", "me"])
        .write_stdin(record("bl-c0de"))
        .assert()
        .success()
        .stderr(contains("rooted in another project").not());
    b.cmd().args(["list", "--json"]).assert().success().stdout(contains("\"id\": \"bl-c0de\""));

    let stream = format!(
        "[{},{},{},{}]",
        record("bl-c0de"), // already held in the store
        record("bl-face"), // genuinely fresh — must NOT survive the refusal
        record("bl-dead"), // first of an intra-stream duplicate pair
        record("bl-dead"),
    );
    b.cmd()
        .args(["import", "--as", "me"])
        .write_stdin(stream)
        .assert()
        .failure()
        .stderr(contains("bl-c0de").and(contains("bl-dead")))
        .stderr(contains("nothing imported"));

    // All-or-nothing: the fresh record was never written; the held one is intact
    // (still resolves, still reports its original verbatim stamps).
    b.cmd().args(["show", "bl-face"]).assert().failure().stderr(contains("bl-face"));
    let held = b.show_json("bl-c0de");
    assert!(held.contains("\"created\": 5") && held.contains("\"updated\": 9"), "held ball untouched: {held}");
}

#[test]
fn minted_ids_match_the_scheme_and_never_duplicate() {
    // `create` mints from the shipped random `IdScheme` (`bl-` + four lower-hex).
    // Every successful create yields a distinct on-disk file, so two successes
    // can never share an id — assert that structurally. A rare birthday
    // collision surfaces as a create FAILURE (finalize finds no new id), not a
    // duplicate; we retry past it so the property holds without flaking.
    let tmp = TempDir::new().unwrap();
    let s = Store::found(tmp.path(), "m");

    let want = 30;
    let mut ids: HashSet<String> = HashSet::new();
    let mut attempts = 0;
    while ids.len() < want && attempts < 200 {
        attempts += 1;
        let out = s.cmd().args(["create", "mint", "--as", "me"]).output().unwrap();
        if !out.status.success() {
            continue; // a random mint collided; the seal rolled back — try again
        }
        let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
        assert!(is_scheme_id(&id), "minted id must be `bl-` + four lower-hex, got {id:?}");
        assert!(ids.insert(id.clone()), "two successful creates minted the same id: {id}");
    }
    assert_eq!(ids.len(), want, "collected {want} distinct minted ids (took {attempts} attempts)");
}

#[test]
fn the_round_trip_reopens_a_closed_ball_in_its_own_store() {
    // The documented substitute for a `reopen` verb (bl-40f5): this store's OWN
    // history is a source like any other, so `show --json | import` restores a
    // retired ball. Nothing is undone — the close commit and the deletion both
    // stand; a ball simply exists again carrying the id and content it had.
    let tmp = TempDir::new().unwrap();
    let s = Store::found(tmp.path(), "reopen");

    let id = created_id(s.cmd().args(["create", "a ball to retire", "-p", "3", "-t", "bug"]).assert().success());
    s.cmd().args(["claim", &id, "--as", "ghost"]).assert().success();
    s.cmd().args(["close", &id, "--as", "ghost"]).assert().success();
    s.cmd().arg("list").assert().success().stdout(contains(&id).not());

    // `show` still resolves the dead id out of history, and that record imports.
    let dead = s.show_json(&id);
    s.cmd().arg("import").write_stdin(dead.clone()).assert().success().stderr(contains("import 1 ball"));

    // Live again, byte-for-byte: verbatim means verbatim, timestamps included.
    s.cmd().arg("list").assert().success().stdout(contains(&id).and(contains("a ball to retire")));
    assert_eq!(s.show_json(&id), dead, "the reopened ball is the record that was read");
    // …and gone from the dead set: one incarnation, and it is live.
    s.cmd().args(["list", "--status", "closed"]).assert().success().stdout(contains(&id).not());

    // The two guards a restore needs, both already ordinary rules. A second
    // import now collides, because the id is live.
    s.cmd().arg("import").write_stdin(dead).assert().failure().stderr(contains("already held"));
    // And the restored claimant is the closer's stale one — `bl unclaim` is the
    // fix, where a `--clean` flag would have been.
    s.cmd().args(["claim", &id, "--as", "alice"]).assert().failure().stderr(contains("already claimed by ghost"));
    s.cmd().args(["unclaim", &id, "--as", "alice"]).assert().success();
    s.cmd().args(["list", "--status", "ready"]).assert().success().stdout(contains(&id));
}
