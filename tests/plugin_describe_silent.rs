//! bl-c343 regression: a clap-built legacy plugin rejects the
//! `describe` subcommand with the clap default "unrecognized
//! subcommand" error. The dispatcher must fall through to the legacy
//! shim per SPEC §12 *silently* — the misleading warning implies
//! something is broken when nothing is.

mod common;

use common::native_plugin::{
    create_auth, install_legacy_plugin, path_with, write_plugin_config,
};
use common::*;

#[test]
fn legacy_plugin_describe_failure_is_silent() {
    let alpha = install_legacy_plugin("alpha");
    let repo = new_repo();
    init_in(repo.path());
    write_plugin_config(repo.path(), &["alpha"]);
    create_auth(repo.path(), "alpha");

    let out = bl(repo.path())
        .env("PATH", path_with(&[alpha.path()]))
        .args(["create", "legacy describe silence"])
        .output()
        .unwrap();
    assert!(out.status.success(), "bl create failed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("describe failed"),
        "legacy plugin must not produce a `describe failed` warning: {stderr}"
    );
    assert!(
        !stderr.contains("unrecognized subcommand"),
        "clap's unrecognized-subcommand error must not leak: {stderr}"
    );
}
