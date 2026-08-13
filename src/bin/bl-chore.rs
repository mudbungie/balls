//! `bl-chore` — the guarded close-gate mint at claim (design bl-3df3), a thin
//! process edge over [`balls::chore`].
//!
//! It answers `protocol` for the §6 self-describe; otherwise it gathers the
//! boundary inputs — op/phase from argv, the §7 wire from stdin, the schedule
//! name from `BALLS_PLUGIN_NAME` (the recursion-break tag + config territory),
//! and the CHANGE WORKTREE from the cwd core invoked it in (§8: a hook runs in
//! the worktree it is invited to edit) — then hands all policy to the library.
//! A non-zero exit aborts the claim (§6); errors go to stderr.
//!
//! Since bl-1da3 there is no `bl` to resolve and no XDG territory to locate: the
//! mint is a write into that worktree rather than a nested op, so the edge reads
//! one env var and one directory.

use std::env;
use std::io::{self, Read};
use std::process::exit;

use balls::chore;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("protocol") {
        println!("{}", chore::PROTOCOL_JSON);
        return;
    }
    if let Err(e) = run(&args) {
        eprintln!("bl-chore: {e}");
        exit(1);
    }
}

/// Gather the boundary inputs and run the hook. op/phase are argv; the wire is
/// stdin; the plugin name is env (defaulting to the conventional `bl-chore`);
/// the change worktree is the cwd. All policy lives in [`balls::chore::run`].
fn run(args: &[String]) -> io::Result<()> {
    let op = args.first().ok_or_else(|| io::Error::other("usage: bl-chore <op> <phase>"))?;
    let phase = args.get(1).ok_or_else(|| io::Error::other("usage: bl-chore <op> <phase>"))?;

    let mut stdin = String::new();
    io::stdin().read_to_string(&mut stdin)?;

    let plugin = env::var("BALLS_PLUGIN_NAME").unwrap_or_else(|_| "bl-chore".to_string());
    chore::run(op, phase, &plugin, &env::current_dir()?, &stdin)
}
