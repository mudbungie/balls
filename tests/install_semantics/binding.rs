//! `bl install --bin` REBIND (the routine plugin upgrade) and the PATH-only
//! resolution tier. Rebinding `ghost=A` → `ghost=B` must make the NEXT dispatch
//! actually RUN B — marker files distinguish A vs B — never stale A; and a
//! landing `bl install` must resolve + bind a referenced plugin found solely on
//! `$PATH` (neither `--bin` nor beside `bl`), the positive of a tier only ever
//! asserted in refusal text (skill/install.md: "beside `bl`, then on `$PATH`").

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use predicates::str::contains;
use tempfile::TempDir;

use crate::{bl, clone_at, primed};

/// A fake plugin that, on a real dispatch (not `protocol`), appends `letter` to
/// `marker` then exits 0 — so a sequence of dispatches records WHICH binary ran.
/// Declares the `create` op so `resolve_and_bind` accepts it on `create.pre`.
fn marker_plugin(path: &Path, letter: &str, marker: &Path) -> PathBuf {
    let m = marker.display();
    let body = format!(
        "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{{\"protocol\":[1],\"ops\":[\"create\"]}}'; exit 0; fi\ncat >/dev/null\necho {letter} >> {m}\nexit 0\n"
    );
    write_exec(path, &body)
}

/// A fake plugin that only self-describes a `list` op and drains stdin — enough
/// for `resolve_and_bind` to accept it on the bare `list` read hook.
fn list_plugin(path: &Path) -> PathBuf {
    let body =
        "#!/bin/sh\nif [ \"$1\" = protocol ]; then printf '{\"protocol\":[1],\"ops\":[\"list\"]}'; exit 0; fi\ncat >/dev/null\nexit 0\n";
    write_exec(path, body)
}

fn write_exec(path: &Path, body: &str) -> PathBuf {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    path.to_path_buf()
}

#[test]
fn rebinding_a_plugin_binary_makes_the_next_dispatch_run_the_new_target() {
    // bl-cff0: `--bin ghost=A` then `--bin ghost=B` is the routine plugin
    // upgrade. The first create dispatches A; after the rebind the SAME hook must
    // dispatch B — the binding replaces, never a stale-A resolve.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed(tmp.path());
    let bins = tmp.path().join("bins");
    fs::create_dir_all(&bins).unwrap();
    let marker = tmp.path().join("which.log");
    let a = marker_plugin(&bins.join("ghost-a"), "A", &marker);
    let b = marker_plugin(&bins.join("ghost-b"), "B", &marker);
    let link = clone_at(&state, &project).landing().join("config/plugins/bin/ghost");

    // Reference ghost on a mutating hook so a bind is legal and create dispatches it.
    bl(&project, &home, &state).args(["conf", "append", "create.pre", "ghost"]).assert().success();

    // Bind ghost → A (bind-only: copies nothing), then dispatch once: A runs.
    bl(&project, &home, &state)
        .args(["install", "--bin", &format!("ghost={}", a.display())])
        .assert()
        .success()
        .stdout(contains("0 added / 0 deleted"));
    bl(&project, &home, &state).args(["create", "one", "--as", "me"]).assert().success();

    // Rebind ghost → B (the upgrade gesture); the link now resolves to B.
    bl(&project, &home, &state).args(["install", "--bin", &format!("ghost={}", b.display())]).assert().success();
    assert_eq!(fs::canonicalize(&link).unwrap(), fs::canonicalize(&b).unwrap(), "the binding now resolves to B");

    // A fresh dispatch runs B, not stale A.
    bl(&project, &home, &state).args(["create", "two", "--as", "me"]).assert().success();
    assert_eq!(fs::read_to_string(&marker).unwrap(), "A\nB\n", "first dispatch ran A, the post-rebind one ran B");
}

#[test]
fn install_resolves_and_binds_a_referenced_plugin_found_only_on_path() {
    // The PATH tier's positive: a plugin neither `--bin` nor beside `bl`, only on
    // `$PATH`. A landing install (`--from` a local ref, so no upstream needed)
    // runs bind_referenced, which misses beside-bl and hits `$PATH` — binding it.
    let tmp = TempDir::new().unwrap();
    let (project, home, state) = primed(tmp.path());
    let pathdir = tmp.path().join("pathdir");
    fs::create_dir_all(&pathdir).unwrap();
    let plugin = list_plugin(&pathdir.join("wanderer"));
    let link = clone_at(&state, &project).landing().join("config/plugins/bin/wanderer");

    // Reference it on a harmless read hook so bind_referenced picks it up.
    bl(&project, &home, &state).args(["conf", "append", "list", "wanderer"]).assert().success();
    assert!(!link.exists(), "unbound before the install");

    // The plugin dir is PREPENDED to `$PATH` (so `git` still resolves too); the
    // landing mirror of its own committed config is a no-op, but the bind runs.
    let path = format!("{}:{}", pathdir.display(), std::env::var("PATH").unwrap());
    bl(&project, &home, &state)
        .args(["install", "config", "--from", "balls/config"])
        .env("PATH", path)
        .assert()
        .success();
    assert!(link.exists(), "the PATH-only plugin was resolved and bound");
    assert_eq!(fs::canonicalize(&link).unwrap(), fs::canonicalize(&plugin).unwrap(), "bound to the $PATH binary");
}
