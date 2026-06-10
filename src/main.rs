//! The `bl` binary: a thin shell over [`balls::run`]. All logic lives in the
//! library (covered by unit tests); `main` only adapts the process boundary —
//! it reads the host environment once (the bl-bfa8 rule: no env reads in the
//! lib) and hands an [`balls::edge::Edge`] in. Integration tests exercise it by
//! running this built binary (see `tests/dispatch.rs`).

use std::env;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::exit;

use balls::edge::Edge;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let edge = Edge::resolve(
        env::var_os("HOME").map(PathBuf::from).unwrap_or_default(),
        env::var("XDG_CONFIG_HOME").ok(),
        env::var("XDG_STATE_HOME").ok(),
        env::current_dir().unwrap_or_default(),
        env::var("USER").ok(),
        env::var("BALLS_PLUGIN_DEPTH").ok(),
        env::current_exe().ok(),
        env::var_os("PATH"),
        env::var("NO_COLOR").ok(),
        std::io::stdout().is_terminal(),
    );
    exit(balls::run(&edge, &args));
}
