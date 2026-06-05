//! The `gate` binary: the default §10 gating plugin, a thin shell over
//! [`balls::gate::run`]. All logic lives in the library (unit-tested); this
//! adapts the process boundary. The plugin runs with cwd = the change worktree,
//! so blockers resolve against `./tasks/` (§6/§10). Integration-tested by
//! running this built binary (see `tests/gate.rs`).

use std::env;
use std::io;
use std::path::Path;
use std::process::exit;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    exit(balls::gate::run(&args, &mut io::stdin().lock(), Path::new(".")));
}
