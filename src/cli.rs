use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "bl", version, about = "Git-native task tracker", long_about = None)]
pub struct Cli {
    /// Force plain output: no color, no Unicode glyphs. Overrides
    /// terminal detection.
    #[arg(long, global = true)]
    pub plain: bool,

    #[command(subcommand)]
    pub command: Command,
}

pub use crate::cli_command::Command;
pub use crate::cli_sub::{DepCmd, LinkCmd, PluginCmd, ShellArg};
