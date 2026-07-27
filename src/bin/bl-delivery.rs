//! `bl-delivery` — the §11 delivery / worktree plugin binary (direct variant).
//!
//! A thin process edge over [`balls::delivery_bin`], the way `bl-tracker` sits
//! over [`balls::tracker::run`]: `main` reads the environment once at the
//! boundary — `$BALLS_PLUGIN_NAME`, the XDG bases, the working directory (the
//! bl-bfa8 rule: the library takes them as arguments) — and hands argv, stdin
//! and stdout to the library entrypoint, which owns the whole boundary
//! adaptation (`protocol`, the §7 wire, the §11 surfacing, the error voice).
//! A linking host multiplexing the plugin calls the same entrypoint, so there
//! is exactly one copy of the boundary.

use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::process::exit;

use balls::delivery_bin;
use balls::layout::Xdg;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let home = env::var("HOME").unwrap_or_default();
    let env = delivery_bin::Env {
        plugin: env::var("BALLS_PLUGIN_NAME").ok(),
        xdg: Xdg::with(Path::new(&home), env::var("XDG_CONFIG_HOME").ok().as_deref(), env::var("XDG_STATE_HOME").ok().as_deref()),
        cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    exit(delivery_bin::run(&args, &mut io::stdin().lock(), &mut io::stdout().lock(), &env));
}
