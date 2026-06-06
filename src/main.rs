//! The `bl` binary: a thin shell over [`balls::run`]. All logic lives in the
//! library (covered by unit tests); `main` only adapts the process boundary,
//! and integration tests exercise it by running this built binary (see
//! `tests/dispatch.rs`).

use std::env;
use std::process::exit;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    exit(balls::run(&args));
}
