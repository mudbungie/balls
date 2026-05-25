//! `bl migrate` — thin CLI wrapper around `balls::migrate::run`. The
//! migration logic itself lives in `balls::migrate` so unit tests can
//! exercise it without spawning a subprocess.

use balls::error::Result;
use std::env;

pub fn cmd_migrate() -> Result<()> {
    let cwd = env::current_dir()?;
    let report = balls::migrate::run(&cwd)?;
    for line in report.lines() {
        println!("{line}");
    }
    Ok(())
}
