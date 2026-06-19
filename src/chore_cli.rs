//! `bl-chore`'s production [`Bl`] seam — shelling the real `bl` binary.
//!
//! [`crate::chore`] is the pure policy (guards + render); this is its one side
//! effect. `bl` is resolved by NAME and found on `$PATH` — the plugin inherits
//! the environment of the `bl` invocation that triggered it (and a user who can
//! run `bl claim` has `bl` on PATH), so a bare name is enough and needs no
//! `current_exe` probing (which has no testable failure to cover). Tests
//! construct [`Cli::at`] against a fake `bl` script.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::chore::Bl;

/// The production [`Bl`]: every chore op is `bl <argv>` run in the project repo.
pub struct Cli {
    bl: PathBuf,
}

impl Cli {
    /// Run against the `bl` at `path` (a bare `"bl"` resolves on `$PATH`; an
    /// absolute path runs that binary — the test seam).
    pub fn at(bl: PathBuf) -> Self {
        Self { bl }
    }
}

impl Bl for Cli {
    fn run(&self, cwd: &Path, argv: &[String]) -> io::Result<String> {
        let out = Command::new(&self.bl).current_dir(cwd).args(argv).output()?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        let verb = argv.first().map_or("", String::as_str);
        Err(io::Error::other(format!("bl {verb} failed: {}", String::from_utf8_lossy(&out.stderr).trim())))
    }
}

#[cfg(test)]
#[path = "chore_cli_tests.rs"]
mod tests;
