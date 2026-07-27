//! `bl-chore` — the guarded close-gate mint at claim (design bl-3df3), a thin
//! process edge over [`balls::chore`].
//!
//! It answers `protocol` for the §6 self-describe; otherwise it gathers the
//! boundary inputs — op/phase from argv, the §7 wire from stdin, the schedule
//! name from `BALLS_PLUGIN_NAME` (the recursion-break tag + config territory),
//! the XDG bases behind this plugin's §1 state territory — and resolves `bl` on
//! `$PATH`, then hands all policy to the library. A non-zero exit aborts the
//! claim (§6); errors go to stderr.

use std::env;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::exit;

use balls::chore;
use balls::chore_cli::Cli;
use balls::layout::Xdg;

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
/// stdin; the plugin name and the XDG bases are env (the name defaulting to the
/// conventional `bl-chore`); `bl` is found on `$PATH`. All policy lives in
/// [`balls::chore::run`] — the layout layer takes its bases as arguments, so this
/// is the one place the environment is read.
fn run(args: &[String]) -> io::Result<()> {
    let op = args.first().ok_or_else(|| io::Error::other("usage: bl-chore <op> <phase>"))?;
    let phase = args.get(1).ok_or_else(|| io::Error::other("usage: bl-chore <op> <phase>"))?;

    let mut stdin = String::new();
    io::stdin().read_to_string(&mut stdin)?;

    let plugin = env::var("BALLS_PLUGIN_NAME").unwrap_or_else(|_| "bl-chore".to_string());
    let home = env::var("HOME").map_err(|_| io::Error::other("HOME is unset (set by balls per §6)"))?;
    let xdg = Xdg::with(Path::new(&home), env::var("XDG_CONFIG_HOME").ok().as_deref(), env::var("XDG_STATE_HOME").ok().as_deref());
    let bl = Cli::at(PathBuf::from("bl"));
    chore::run(op, phase, &plugin, &xdg.plugin_territory(&plugin), &stdin, &bl)
}
