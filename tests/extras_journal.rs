//! End-to-end for three authoring promises the binary tests never drove:
//!   * the `key=value` **preserved-extras seam** (§3) — set / clear / reserved
//!     refusal / import-round-trip, all observed through `show --json`;
//!   * the **`-m`-only progress note** — a zero-field update always commits and
//!     journals oldest-first, and a SECOND note pinned to the SAME wall-clock
//!     second aborts LOUDLY rather than dropping it (the loud-loss guard, bl-cf93);
//!   * `import`'s **stdin grammar** — malformed JSON fails cleanly, names the
//!     problem, leaves the store untouched, and NEVER panics.
//!
//! Every assertion rides the observable surface (stdout/stderr/exit + `show`),
//! never internals. Each test runs the freshly-built `bl` against a throwaway
//! project repo on `main` under a pinned `HOME`/state, so no store touches the
//! real `$HOME`. The same-second story pins the op instant through the
//! `BALLS_CLOCK` edge seam (src/clock.rs: an `i64` env read once per op).

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// One isolated `bl` world: a project repo on `main` plus the `HOME`/state its
/// store lands under, all inside a `TempDir`.
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
    fn found(tmp: &Path) -> Store {
        let s = Store { project: tmp.join("p"), home: tmp.join("h"), state: tmp.join("s") };
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

    /// A fresh, fully-isolated `bl` invocation rooted in this store's project —
    /// pinned `HOME`/state, no host `$XDG_CONFIG_HOME`, and the inherited
    /// plugin-chain env scrubbed (this target may itself run inside a hook).
    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("bl").unwrap();
        c.current_dir(&self.project)
            .env("HOME", &self.home)
            .env("XDG_STATE_HOME", &self.state)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("BALLS_CLOCK")
            .env_remove("BALLS_PLUGIN_DEPTH")
            .env_remove("BALLS_PLUGIN_NAME");
        c
    }

    /// Like [`cmd`], but pins the op instant to `t` seconds through the
    /// `BALLS_CLOCK` edge seam — two ops sharing `t` fall in the same second.
    fn cmd_at(&self, t: i64) -> Command {
        let mut c = self.cmd();
        c.env("BALLS_CLOCK", t.to_string());
        c
    }

    /// This store's `show <id> --json` bytes — the lossless bedrock record.
    fn show_json(&self, id: &str) -> String {
        let out = self.cmd().args(["show", id, "--json"]).assert().success();
        String::from_utf8(out.get_output().stdout.clone()).unwrap()
    }

    /// This store's human `show <id>` bytes — the journal-rendering projection.
    fn show(&self, id: &str) -> String {
        let out = self.cmd().args(["show", id]).assert().success();
        String::from_utf8(out.get_output().stdout.clone()).unwrap()
    }
}

/// The id `bl create` printed alone to stdout (§9).
fn created_id(out: assert_cmd::assert::Assert) -> String {
    String::from_utf8(out.get_output().stdout.clone()).unwrap().trim().to_string()
}

#[test]
fn the_extras_seam_sets_clears_refuses_reserved_and_survives_import() {
    // §3 preserved-extras: an unknown `key=value` positional round-trips through
    // the bedrock `--json` verbatim; a bare `key=` clears it; a RESERVED key is
    // refused BY NAME with the store unmutated; and an imported record carrying
    // an extra reproduces it byte-for-byte — every path on the observable surface.
    let tmp = TempDir::new().unwrap();
    let s = Store::found(tmp.path());
    let id = created_id(s.cmd().args(["create", "Extras", "--as", "me"]).assert().success());

    // Set: `jira=PROJ-42` lands as a top-level bedrock key (extras flatten in).
    s.cmd().args(["update", &id, "jira=PROJ-42", "--as", "me"]).assert().success();
    assert!(s.show_json(&id).contains("\"jira\": \"PROJ-42\""), "extra must round-trip through --json");

    // A RESERVED key is refused by name, both a bare id and a stamp — and the
    // still-present `jira` proves nothing was mutated on the way to the refusal.
    for kv in ["id=x", "created=x"] {
        s.cmd()
            .args(["update", &id, kv, "--as", "me"])
            .assert()
            .failure()
            .stderr(contains("is reserved, not an extra"));
    }
    assert!(s.show_json(&id).contains("\"jira\": \"PROJ-42\""), "a refused reserved key mutates nothing");

    // Clear: a bare `jira=` removes the key entirely (not sets it to "").
    s.cmd().args(["update", &id, "jira=", "--as", "me"]).assert().success();
    assert!(!s.show_json(&id).contains("jira"), "a bare key= removes the extra");

    // An imported record carrying an extra preserves it verbatim through the
    // write inverse of the bedrock read (§16).
    let rec = r#"{"id":"bl-aa11","title":"imp","created":5,"updated":9,"body":"b\n","team":"payments"}"#;
    s.cmd().args(["import", "--as", "me"]).write_stdin(rec).assert().success();
    assert!(s.show_json("bl-aa11").contains("\"team\": \"payments\""), "import preserves an extra verbatim");
}

#[test]
fn m_only_notes_journal_oldest_first_and_a_same_second_repeat_aborts_loudly() {
    // The agent progress-note gesture: `update <id> -m` with NO field flags is a
    // zero-edit update that still commits (the `updated` restamp) and renders in
    // `bl show`'s journal oldest-first. Then the loud-loss guard (bl-cf93): a
    // second `-m`-only update pinned to the SAME second would stage a
    // byte-identical tree — the docs promise it FAILS rather than silently drop
    // the note. This holds (works-as-designed) — pinned via BALLS_CLOCK.
    let tmp = TempDir::new().unwrap();
    let s = Store::found(tmp.path());
    let id = created_id(s.cmd().args(["create", "Journal", "--as", "me"]).assert().success());

    // Two zero-field notes on DISTINCT seconds both commit.
    s.cmd_at(1_700_000_000).args(["update", &id, "-m", "the-older-note", "--as", "me"]).assert().success();
    s.cmd_at(1_700_000_050).args(["update", &id, "-m", "the-newer-note", "--as", "me"]).assert().success();

    // Both render, oldest-first: the older note precedes the newer in the journal.
    let shown = s.show(&id);
    let older = shown.find("the-older-note").expect("older note journals");
    let newer = shown.find("the-newer-note").expect("newer note journals");
    assert!(older < newer, "journal renders oldest-first:\n{shown}");

    // The loud-loss guard: same-second second `-m` aborts, naming the loss —
    // never a silent no-op. `unaffected-loser` must never reach the store.
    s.cmd_at(1_700_000_100).args(["update", &id, "-m", "kept-first", "--as", "me"]).assert().success();
    s.cmd_at(1_700_000_100)
        .args(["update", &id, "-m", "unaffected-loser", "--as", "me"])
        .assert()
        .failure()
        .stderr(contains("nothing changed").and(contains("would be lost")));

    // The dropped note left no trace; the surviving same-second note is intact.
    let after = s.show(&id);
    assert!(!after.contains("unaffected-loser"), "the aborted note never journaled:\n{after}");
    assert!(after.contains("kept-first"), "the first same-second note survives:\n{after}");
}

#[test]
fn malformed_import_stdin_fails_cleanly_without_touching_the_store_or_panicking() {
    // §16 refuse-don't-guess on the read inverse: four broken streams — non-JSON,
    // a record missing a required field, an object with no id, and an array of
    // scalars — each fails cleanly (naming the problem, nonzero exit), leaves the
    // store's prior count unchanged, and NEVER panics (the grammar is total, not
    // a `.unwrap()` minefield). Assert stderr never carries "panicked".
    let tmp = TempDir::new().unwrap();
    let s = Store::found(tmp.path());

    // Seed one good ball so "store untouched" is a count that must not move.
    let rec = r#"{"id":"bl-c0de","title":"seed","created":5,"updated":9,"body":""}"#;
    s.cmd().args(["import", "--as", "me"]).write_stdin(rec).assert().success();
    s.cmd().args(["list", "--json"]).assert().success().stdout(contains("bl-c0de").count(1));

    // Each malformed stream: nonzero exit, a naming diagnostic, and no panic.
    let cases: [(&str, &str); 4] = [
        ("not json", "bad JSON"),                    // not parseable at all
        (r#"{"id":"bl-x"}"#, "missing field"),       // valid id, required field absent
        (r#"{"title":"no id"}"#, "needs an \"id\""), // an object with no identity
        ("[1,2,3]", "must be a JSON object"),        // array of non-object scalars
    ];
    for (stdin, needle) in cases {
        s.cmd()
            .args(["import", "--as", "me"])
            .write_stdin(stdin)
            .assert()
            .failure()
            .stderr(contains(needle).and(contains("panicked").not()));
    }

    // The store is exactly as seeded — no partial write from any broken stream.
    s.cmd().args(["list", "--json"]).assert().success().stdout(contains("bl-c0de").count(1));
    assert!(!s.show_json("bl-c0de").contains("bl-x"), "no malformed id leaked into the store");
}
