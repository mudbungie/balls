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
        std::fs::write(s.project.join("seed.txt"), "x").unwrap();
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
    b.cmd()
        .args(["import", "--as", "me"])
        .write_stdin(record.clone())
        .assert()
        .success()
        .stdout("")
        .stderr(contains("import 1 ball"));

    // Byte-equivalent record on the far side: nothing was minted or restamped.
    assert_eq!(record, b.show_json(&id), "imported ball must round-trip byte-for-byte in show --json");
    // And the id survived verbatim rather than a fresh mint being substituted.
    assert!(record.contains(&format!("\"id\": \"{id}\"")), "the source id is preserved: {record}");
    // Nothing extra minted: B holds exactly the one imported id.
    b.cmd().args(["list", "--json"]).assert().success().stdout(contains(&id[..]).count(1));
}

#[test]
fn a_colliding_stream_is_refused_wholesale_before_any_write() {
    // Seed B with a held id, then hand import a stream that both re-imports that
    // id AND repeats a fresh one within the stream. Refuse-don't-guess (§16):
    // the WHOLE stream aborts before a byte is written, exit nonzero, naming
    // every offending id — so the genuinely-fresh record between them is dropped.
    let tmp = TempDir::new().unwrap();
    let b = Store::found(tmp.path(), "b");
    b.cmd().args(["import", "--as", "me"]).write_stdin(record("bl-c0de")).assert().success();

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
