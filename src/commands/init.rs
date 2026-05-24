//! `bl init` — working-tree setup and the `--bare` workspace
//! bootstrap. Split from `basic.rs` so both stay under the line cap
//! and the bare path has a clear home.

use balls::error::{BallError, Result};
use balls::store::Store;
use std::env;
use std::path::Path;

pub fn cmd_init(
    stealth: bool,
    tasks_dir: Option<String>,
    bare: Option<Vec<String>>,
) -> Result<()> {
    if let Some(args) = bare {
        if stealth || tasks_dir.is_some() {
            return Err(BallError::Other(
                "--bare cannot be combined with --stealth or --tasks-dir".into(),
            ));
        }
        // clap enforces num_args = 2, so exactly two values are present.
        let store = Store::init_bare(&args[0], Path::new(&args[1]))?;
        println!("Initialized bare balls workspace in {}", store.root.display());
        return Ok(());
    }
    let cwd = env::current_dir()?;
    let store = Store::init(&cwd, stealth, tasks_dir)?;
    if store.stealth {
        println!("Initialized balls (stealth) in {}", store.root.display());
        println!("Tasks stored at: {}", store.tasks_dir().display());
    } else {
        println!("Initialized balls in {}", store.root.display());
    }
    Ok(())
}
