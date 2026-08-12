//! `bl-speculate` — the verdict-cache edge (bl-1263, design
//! docs/design/bl-24e7-speculative-merge-queue.md), a thin process boundary
//! over [`balls::speculate`].
//!
//! Unlike the §6 plugins this binary is NOT dispatched by `bl`: its caller is
//! `scripts/pre-commit`, which consults `check` before running the gates and
//! `record pass` after they pass. It follows the sibling-binary convention all
//! the same — gather the boundary inputs here (cwd as the repo root, the XDG
//! bases behind the §1 `bl-speculate` territory, `BALLS_IDENTITY` as the
//! builder, `rustc -V` as the toolchain half of the gate fingerprint) and hand
//! every decision to the library.
//!
//! Exit codes are the hook's interface: `0` check-hit / record-ok, `3` an
//! honest check miss, `1` any error. The hook treats every non-zero the same —
//! run the stock gate — so all failure is fail-open by construction.

use std::env;
use std::io;
use std::path::Path;
use std::process::{exit, Command};

use balls::layout::Xdg;
use balls::speculate;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => {}
        Ok(false) => exit(3),
        Err(e) => {
            eprintln!("bl-speculate: {e}");
            exit(1);
        }
    }
}

/// Gather the environment once, then dispatch. `Ok(false)` is only ever a
/// check miss; every other completion is `Ok(true)` or an error.
fn run(args: &[String]) -> io::Result<bool> {
    let home = env::var("HOME").map_err(|_| io::Error::other("HOME is unset"))?;
    let xdg = Xdg::with(
        Path::new(&home),
        env::var("XDG_CONFIG_HOME").ok().as_deref(),
        env::var("XDG_STATE_HOME").ok().as_deref(),
    );
    let territory = xdg.plugin_territory("bl-speculate");
    let scratch = territory.join("scratch");
    let root = env::current_dir()?;
    let toolchain = toolchain()?;
    match args.first().map(String::as_str) {
        Some("check") => speculate::check(&root, &scratch, &territory, &toolchain),
        Some("record") => {
            let pass = match args.get(1).map(String::as_str) {
                Some("pass") => true,
                Some("fail") => false,
                _ => return Err(io::Error::other("usage: bl-speculate record pass|fail")),
            };
            let builder = env::var("BALLS_IDENTITY").unwrap_or_else(|_| "local".to_string());
            speculate::record(&root, &scratch, &territory, &toolchain, pass, &builder)?;
            Ok(true)
        }
        _ => Err(io::Error::other("usage: bl-speculate check|record pass|fail")),
    }
}

/// `rustc -V` — the toolchain half of the gate fingerprint. Shelled here, not
/// in the library, so the library stays deterministic under test.
fn toolchain() -> io::Result<String> {
    let out = Command::new("rustc").arg("-V").output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(io::Error::other("rustc -V failed"))
    }
}
