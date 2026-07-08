//! §6/§8 bind surface (bl-98ba): the `clock_provider` is a bindable name and a
//! BIND-ONLY install (no config source) on a stealth box, driven end to end
//! through [`super::run`] with the shared [`super::tests`] fixtures. The
//! clock-provider binding half lives in [`super::bind`]; these exercise it.

#![cfg(unix)]

use super::tests::{edge, found, head, op_log, run_install};
use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

/// An executable fake `clock_provider`: runs `body` (expected to print one line)
/// and exits 0 — NO `protocol` handshake, since a provider resolves an INPUT
/// (the op clock, §8), not an effect, so it is validated as a clock not a plugin.
fn clock_bin(dir: &Path, name: &str, body: &str) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Name `clock_provider = <name>` in the per-machine XDG config layer (§4) — the
/// durable home a stealth box declares its provider in, needing no config source.
fn set_clock_provider(e: &Edge, name: &str) {
    let uc = e.xdg.user_config();
    fs::create_dir_all(uc.parent().unwrap()).unwrap();
    fs::write(uc, format!("clock_provider = \"{name}\"\n")).unwrap();
}

#[test]
fn a_configured_clock_provider_binds_bind_only_with_no_config_source() {
    // §6/§8 (bl-98ba): a stealth landing (no upstream) with a configured
    // `clock_provider` and `--bin <provider>=<path>` runs BIND-ONLY — no config
    // is copied and nothing seals (the tip stands), but the provider IS
    // validated as a clock (one unix-seconds line, exit 0) and linked. This is
    // the "bound by bl install --bin" path the clock docs promised, now real.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    set_clock_provider(&e, "clk");
    let bin = clock_bin(&tmp.path().join("elsewhere"), "clk", "echo 1700000000");
    let before = head(&landing);

    run_install(&e, &["--bin", &format!("clk={}", bin.display())]).unwrap();

    assert_eq!(fs::read_link(landing.join("config/plugins/bin/clk")).unwrap(), bin);
    assert_eq!(before, head(&landing), "bind-only: 0 added / 0 deleted, nothing sealed");
}

#[test]
fn a_bin_naming_neither_a_hook_nor_the_clock_provider_is_refused() {
    // The `--bin` guard still refuses an unknown name: a scheduled plugin OR the
    // configured `clock_provider` are the only bindable names — an unrelated one
    // is refused, never silently dropped (bl-98ba, the bl-cf93 discipline).
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let _ = found(&e);
    set_clock_provider(&e, "clk"); // clk would be accepted; ghost is neither
    let err = run_install(&e, &["--bin", "ghost=/nope"]).unwrap_err();
    assert!(err.to_string().contains("does not reference that plugin"), "{err}");
}

#[test]
fn a_clock_provider_that_is_not_a_clock_is_refused_not_bound() {
    // Validated as a CLOCK, not a plugin (bl-98ba): a provider whose bin prints
    // a non-integer is REFUSED — never linked — so a bad provider cannot slip in
    // to stamp garbage. The bl-8b98 fail-open ladder then degrades op T to the
    // system clock at run-time; bind refuses up front.
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    set_clock_provider(&e, "clk");
    let bin = clock_bin(&tmp.path().join("elsewhere"), "clk", "echo not-a-number");

    let err = run_install(&e, &["--bin", &format!("clk={}", bin.display())]).unwrap_err();

    assert!(err.to_string().contains("refusing to bind clock_provider clk"), "{err}");
    assert!(!landing.join("config/plugins/bin/clk").exists(), "a non-clock is never bound");
}

#[test]
fn an_unbound_clock_provider_dangles_and_the_op_still_succeeds() {
    // A configured provider resolvable to no binary stays dangling — the SAME
    // "referenced but not bound" info line the plugins use, its `[source]` hint
    // appended — and the install still SUCCEEDS: an unresolvable clock is
    // fail-open, not a hard error (bl-98ba/bl-8b98).
    let tmp = TempDir::new().unwrap();
    let e = edge(&tmp, None);
    let (landing, _store) = found(&e);
    set_clock_provider(&e, "clk");
    fs::write(e.xdg.user_config().with_file_name("plugins.toml"), "[source]\nclk = \"cargo install clk\"\n").unwrap();

    run_install(&e, &[]).unwrap(); // bare: no --from, no --bin, but a configured clock ⇒ bind-only

    assert!(!landing.join("config/plugins/bin/clk").exists(), "nothing to bind");
    let log = op_log(&e);
    assert!(
        log.contains("install: clk referenced but not bound (no binary beside bl or on PATH) — source: cargo install clk — re-run bl install after acquiring"),
        "{log}"
    );
}
