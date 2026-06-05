//! The `tracker` plugin binary: a thin shell over [`balls::tracker`].
//!
//! All logic lives in the library (unit-tested on throwaway repos); `main` only
//! adapts the process boundary — it reads the XDG environment once at the edge
//! (the bl-bfa8 rule: no env reads in the lib) and hands the rest to
//! [`balls::tracker::run`]. The integration test (`tests/tracker.rs`) exercises
//! this built binary end to end.

use std::env;
use std::io;
use std::path::PathBuf;
use std::process::exit;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let config_home = env::var("XDG_CONFIG_HOME").ok();
    let state_home = env::var("XDG_STATE_HOME").ok();
    let xdg = balls::layout::Xdg::with(&home, config_home.as_deref(), state_home.as_deref());
    let env = balls::tracker::Env { xdg };
    let code = balls::tracker::run(&args, &mut io::stdin().lock(), &mut io::stdout().lock(), &env);
    exit(code);
}
