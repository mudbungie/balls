//! End-to-end: the bl-5b09 `[source]` acquisition hints and the two honesty
//! fixes, asserted on the real binary with the exact design-doc strings.
//!
//! Doctrine (docs/design/bl-5b09-capability-distribution.md, CONVERGED
//! 2026-07-04): distribution is the package manager's job — balls ships a
//! pointer, not a pipeline. A `[source]` hint is free text displayed VERBATIM
//! at the refusal moments core already emits; nothing is parsed, fetched, or
//! executed, and deleting every hint yields bit-identical behavior with terser
//! errors. These tests drive each decorated moment through the shipped `bl`:
//! the dispatch unbound abort, install's dangling report and validation
//! refusal, the `bl conf` unbound section, and the seed-prune note.

#![cfg(unix)]

use assert_cmd::Command;
use predicates::boolean::PredicateBooleanExt;
use predicates::str::contains;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `bl` rooted in `project`, `HOME`/`$XDG_STATE_HOME` pinned under the tempdir
/// (the `tests/install_recovery.rs` harness). Stealth box: no remote is ever
/// contacted, so the XDG config layer is `home/.config/balls/`.
fn bl(project: &Path, home: &Path, state: &Path) -> Command {
    let mut cmd = Command::cargo_bin("bl").unwrap();
    cmd.current_dir(project)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env_remove("XDG_CONFIG_HOME");
    cmd
}

/// One primed throwaway project; returns (tempdir, project, home, state).
fn primed() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    fs::create_dir_all(&project).unwrap();
    bl(&project, &home, &state).arg("prime").assert().success();
    (tmp, project, home, state)
}

/// Author `[source]` hints on the per-machine XDG layer (§4: the same file the
/// dispatch already merges, innermost wins) — `home/.config/balls/plugins.toml`.
fn author_hints(home: &Path, body: &str) {
    let dir = home.join(".config").join("balls");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("plugins.toml"), body).unwrap();
}

/// The landing→landing self-copy — the cheapest real install (no-op seal).
const SELF_COPY: [&str; 6] = ["install", "config", "--from", "balls/config", "--to", "balls/config"];

/// A real, bindable plugin answering `<bin> protocol` with `ops`.
fn fake_plugin(dir: &Path, name: &str, ops: &str) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    let body =
        format!("#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{{\"protocol\":[1],\"ops\":{ops}}}'; exit 0; fi\ncat >/dev/null\nexit 0\n");
    fs::write(&path, body).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn the_dispatch_unbound_abort_names_the_source_hint() {
    // §2(a) of the design: same refusal, more words — the hint, then the bind
    // step it does not replace.
    let (_tmp, project, home, state) = primed();
    author_hints(&home, "[source]\nghost = \"cargo install ghost\"\n");
    bl(&project, &home, &state).args(["conf", "append", "install.pre", "ghost"]).assert().success();

    bl(&project, &home, &state).args(SELF_COPY).assert().failure().stderr(contains(
        "plugin ghost referenced but bin/ghost missing — source: cargo install ghost — then bl install to bind",
    ));
}

#[test]
fn install_reports_dangling_names_and_conf_grows_the_unbound_section() {
    // Honesty fixes §3: the Summary no longer reads as "covered everything" —
    // each referenced-but-unbound name gets one info line (hint appended when
    // authored), and the conf dump grows an `unbound` row per dangling name.
    let (_tmp, project, home, state) = primed();

    // All bound (the freshly primed default set) ⇒ the section is ABSENT.
    bl(&project, &home, &state).arg("conf").assert().success().stdout(contains("unbound").not());

    author_hints(&home, "[source]\nghost = \"cargo install ghost\"\n");
    bl(&project, &home, &state).args(["conf", "append", "claim.post", "ghost"]).assert().success();
    bl(&project, &home, &state).args(["conf", "append", "claim.post", "mute"]).assert().success();

    bl(&project, &home, &state).args(SELF_COPY).assert().success()
        .stderr(contains(
            "install: ghost referenced but not bound (no binary beside bl or on PATH) — source: cargo install ghost — re-run bl install after acquiring",
        ))
        .stderr(contains(
            "install: mute referenced but not bound (no binary beside bl or on PATH) — re-run bl install after acquiring",
        ));

    bl(&project, &home, &state).arg("conf").assert().success()
        .stdout(contains("unbound").and(contains("cargo install ghost")).and(contains("(no source given)")));
}

#[test]
fn the_validation_refusal_appends_the_hint_as_an_upgrade_pointer() {
    // §2(b): a binary that does not speak for the op it is wired into refuses
    // with the hint appended — the stale-binary upgrade pointer.
    let (tmp, project, home, state) = primed();
    author_hints(&home, "[source]\nghost = \"cargo install ghost\"\n");
    bl(&project, &home, &state).args(["conf", "append", "claim.post", "ghost"]).assert().success();
    let bin = fake_plugin(&tmp.path().join("bins"), "ghost", r#"["close"]"#);

    let mut args: Vec<String> = SELF_COPY.iter().map(ToString::to_string).collect();
    args.extend(["--bin".into(), format!("ghost={}", bin.display())]);
    bl(&project, &home, &state).args(&args).assert().failure().stderr(contains(
        "install: refusing to link ghost: does not handle op 'claim' — source: cargo install ghost",
    ));
}

#[test]
fn the_seed_prune_is_loud_only_for_a_hinted_name() {
    // §2(c): loudness keyed on hint presence — the org opted in by authoring
    // it; the shipped-sibling prune (hintless) stays silent.
    let tmp = TempDir::new().unwrap();
    let (home, state, project) = (tmp.path().join("h"), tmp.path().join("s"), tmp.path().join("p"));
    fs::create_dir_all(&project).unwrap();
    let dc = home.join(".config").join("balls").join("default-config");
    fs::create_dir_all(&dc).unwrap();
    fs::write(
        dc.join("plugins.toml"),
        "[hooks]\n\"close.pre\" = [\"ghost\", \"mute\", \"bl-delivery\"]\n\"close.post\" = [\"bl-delivery\", \"bl-tracker\"]\n[source]\nghost = \"cargo install ghost\"\n",
    )
    .unwrap();

    bl(&project, &home, &state).arg("prime").assert().success()
        .stderr(contains(
            "seed: pruned ghost (no binary beside bl) — source: cargo install ghost — re-add with bl conf after acquiring",
        ))
        .stderr(contains("pruned mute").not());
}
