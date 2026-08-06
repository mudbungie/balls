//! Tests for the landing's brief (bl-c84f). The contract is three sentences:
//! a landing WITH `config/PRIME.md` reads back its exact bytes, a landing
//! without one is silent (`None`, never an error), and what `emit` puts on
//! stdout is verbatim — no header, no wrapper, not even a newline of ours.

use crate::brief;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// A bare landing — just the `config/` dir the brief lives in. No founding
/// needed: `read` is a file probe, and giving it less than a real landing is
/// the point (it must not depend on anything else being seeded).
fn landing(tmp: &TempDir) -> PathBuf {
    let landing = tmp.path().join("config-branch");
    fs::create_dir_all(landing.join("config")).unwrap();
    landing
}

#[test]
fn absent_brief_is_silence_not_an_error() {
    let tmp = TempDir::new().unwrap();
    // The ordinary case: no brief configured. `None`, not `Err` and not `""` —
    // "this landing has nothing to say" is a state, not a failure, and the
    // no-seed decision makes it the DEFAULT state of every fresh landing.
    assert_eq!(brief::read(&landing(&tmp)).unwrap(), None);
}

#[test]
fn a_configured_brief_reads_back_verbatim() {
    let tmp = TempDir::new().unwrap();
    let landing = landing(&tmp);
    // Deliberately shaped like a real brief: a POINTER, not a restatement.
    let text = "# balls\n\nRead `docs/architecture.md` §9 before touching close.\n";
    fs::write(landing.join("config").join("PRIME.md"), text).unwrap();
    assert_eq!(brief::read(&landing).unwrap().as_deref(), Some(text));
}

#[test]
fn a_brief_with_no_trailing_newline_survives_intact() {
    let tmp = TempDir::new().unwrap();
    let landing = landing(&tmp);
    // Verbatim is the whole promise, so the file owns its own shape — core
    // adds no trailing newline to tidy it up. If this ever starts passing with
    // a "\n" appended, `emit` has grown an opinion it is not allowed to have.
    fs::write(landing.join("config").join("PRIME.md"), "no trailing newline").unwrap();
    assert_eq!(brief::read(&landing).unwrap().as_deref(), Some("no trailing newline"));
}

#[test]
fn emit_prints_a_brief_and_no_ops_without_one() {
    let tmp = TempDir::new().unwrap();
    let landing = landing(&tmp);
    // Both arms of the only branch `emit` has. The bare landing first (nothing
    // to print), then the configured one — stdout goes to the test harness,
    // which is exactly the point: emitting must not be able to fail on a
    // landing that has a brief, nor on one that does not.
    brief::emit(&landing).unwrap();
    fs::write(landing.join("config").join("PRIME.md"), "brief\n").unwrap();
    brief::emit(&landing).unwrap();
}
