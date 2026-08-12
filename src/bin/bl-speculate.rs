//! `bl-speculate` — the merge-queue edge (design
//! docs/design/bl-24e7-speculative-merge-queue.md), a thin process boundary
//! over [`balls::speculate`] (the verdict cache, bl-1263) and
//! [`balls::speculate_queue`] (the merging queue, bl-5c5f).
//!
//! Unlike the §6 plugins this binary is NOT dispatched by `bl`: its callers
//! are `scripts/pre-commit` (`check` before the gates, `record pass` after)
//! and queue-driving agents (`enqueue`/`dequeue`/`queue`). It follows the
//! sibling-binary convention all the same — gather the boundary inputs here
//! (cwd as the repo root; for the cache verbs the XDG bases behind the §1
//! `bl-speculate` territory, `BALLS_IDENTITY` as the builder and `rustc -V`
//! as the toolchain half of the gate fingerprint) and hand every decision to
//! the library. The queue verbs deliberately read no environment at all: a
//! queue query must not fail for a cache-side reason.
//!
//! Exit codes are the interface: `0` check-hit / verb-ok, `3` an honest check
//! miss, `1` any error. The hook treats every non-zero the same — run the
//! stock gate — so all cache failure is fail-open by construction.

use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

use balls::layout::Xdg;
use balls::{speculate, speculate_queue};

const USAGE: &str = "usage: bl-speculate check | record pass|fail | enqueue ID | dequeue ID | queue";

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

/// Dispatch. `Ok(false)` is only ever a check miss; every other completion is
/// `Ok(true)` or an error.
fn run(args: &[String]) -> io::Result<bool> {
    let root = env::current_dir()?;
    match args.first().map(String::as_str) {
        Some("check") => {
            let (territory, scratch) = territory()?;
            speculate::check(&root, &scratch, &territory, &toolchain()?)
        }
        Some("record") => {
            let pass = match args.get(1).map(String::as_str) {
                Some("pass") => true,
                Some("fail") => false,
                _ => return Err(io::Error::other(USAGE)),
            };
            let (territory, scratch) = territory()?;
            let builder = env::var("BALLS_IDENTITY").unwrap_or_else(|_| "local".to_string());
            speculate::record(&root, &scratch, &territory, &toolchain()?, pass, &builder)?;
            Ok(true)
        }
        Some("enqueue") => {
            println!("{}", speculate_queue::enqueue(&root, id_arg(args)?, None)?);
            Ok(true)
        }
        Some("dequeue") => {
            speculate_queue::dequeue(&root, id_arg(args)?)?;
            Ok(true)
        }
        Some("queue") => {
            let mut pos = 0;
            for e in speculate_queue::queue(&root)? {
                if e.sealed {
                    pos += 1;
                    println!("{pos} {} {}", e.id, e.tip);
                } else {
                    println!("- {} {} unsealed", e.id, e.tip);
                }
            }
            Ok(true)
        }
        _ => Err(io::Error::other(USAGE)),
    }
}

/// The queue verbs' second word — the ball id.
fn id_arg(args: &[String]) -> io::Result<&str> {
    args.get(1).map(String::as_str).ok_or_else(|| io::Error::other(USAGE))
}

/// The §1 cache territory and its scratch dir — read from the environment
/// only when a cache verb actually needs them.
fn territory() -> io::Result<(PathBuf, PathBuf)> {
    let home = env::var("HOME").map_err(|_| io::Error::other("HOME is unset"))?;
    let xdg = Xdg::with(
        Path::new(&home),
        env::var("XDG_CONFIG_HOME").ok().as_deref(),
        env::var("XDG_STATE_HOME").ok().as_deref(),
    );
    let territory = xdg.plugin_territory("bl-speculate");
    let scratch = territory.join("scratch");
    Ok((territory, scratch))
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
