mod cli;
mod commands;

use balls::error::{BallError, Result};
use clap::{CommandFactory, Parser};
use cli::{Cli, Command, DepCmd, LinkCmd, ShellArg};

/// Accept bare-hex task ids on the CLI: `534c` becomes `bl-534c`. Anything
/// already prefixed or not pure hex is returned unchanged, so the storage
/// layer keeps seeing full ids.
fn normalize_id(s: String) -> String {
    if !s.is_empty() && !s.starts_with("bl-") && s.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("bl-{s}")
    } else {
        s
    }
}

fn normalize_opt(o: Option<String>) -> Option<String> {
    o.map(normalize_id)
}

fn normalize_vec(v: Vec<String>) -> Vec<String> {
    v.into_iter().map(normalize_id).collect()
}

fn normalize_dep(sub: DepCmd) -> DepCmd {
    match sub {
        DepCmd::Add { task, depends_on } => DepCmd::Add {
            task: normalize_id(task),
            depends_on: normalize_id(depends_on),
        },
        DepCmd::Rm { task, depends_on } => DepCmd::Rm {
            task: normalize_id(task),
            depends_on: normalize_id(depends_on),
        },
        DepCmd::Tree { id, json } => DepCmd::Tree {
            id: normalize_opt(id),
            json,
        },
    }
}

fn normalize_link(sub: LinkCmd) -> LinkCmd {
    match sub {
        LinkCmd::Add {
            task,
            link_type,
            target,
        } => LinkCmd::Add {
            task: normalize_id(task),
            link_type,
            target: normalize_id(target),
        },
        LinkCmd::Rm {
            task,
            link_type,
            target,
        } => LinkCmd::Rm {
            task: normalize_id(task),
            link_type,
            target: normalize_id(target),
        },
    }
}

fn home_path() -> Result<std::path::PathBuf> {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .map_err(|_| BallError::Other("HOME not set".into()))
}

fn handle_completions(shell: Option<ShellArg>, install: bool, uninstall: bool) -> Result<()> {
    let mut cmd = Cli::command();
    if install {
        let home = home_path()?;
        for p in commands::install_completions(&mut cmd, &home)? {
            println!("installed {}", p.display());
        }
        Ok(())
    } else if uninstall {
        let home = home_path()?;
        for p in commands::uninstall_completions(&home)? {
            println!("removed {}", p.display());
        }
        Ok(())
    } else if let Some(shell) = shell {
        let shell = match shell {
            ShellArg::Bash => clap_complete::Shell::Bash,
            ShellArg::Zsh => clap_complete::Shell::Zsh,
            ShellArg::Fish => clap_complete::Shell::Fish,
        };
        clap_complete::generate(shell, &mut cmd, "bl", &mut std::io::stdout());
        Ok(())
    } else {
        Err(BallError::Other(
            "specify a shell (bash|zsh|fish), --install, or --uninstall".into(),
        ))
    }
}

fn main() {
    let cli = Cli::parse();
    balls::display::init(cli.plain);
    let result = match cli.command {
        Command::Init { stealth, tasks_dir } => commands::cmd_init(stealth, tasks_dir),
        Command::Create {
            title,
            priority,
            task_type,
            parent,
            dep,
            tag,
            description,
        } => commands::cmd_create(
            title,
            priority,
            task_type,
            normalize_opt(parent),
            normalize_vec(dep),
            tag,
            description,
        ),
        Command::List {
            status,
            priority,
            parent,
            tag,
            all,
            json,
        } => commands::cmd_list(status, priority, normalize_opt(parent), tag, all, json),
        Command::Show { id, json, verbose } => commands::cmd_show(normalize_id(id), json, verbose),
        Command::Ready { json, no_fetch, limit } => commands::cmd_ready(json, no_fetch, limit),
        Command::Claim { id, identity, no_worktree, sync, no_sync } => {
            commands::cmd_claim(normalize_id(id), identity, no_worktree, sync, no_sync)
        }
        Command::Review { id, message, identity, sync, no_sync } => {
            commands::cmd_review(normalize_id(id), message, identity, sync, no_sync)
        }
        Command::Close { id, message, identity, sync, no_sync } => {
            commands::cmd_close(normalize_id(id), message, identity, sync, no_sync)
        }
        Command::Drop { id, force } => commands::cmd_drop(normalize_id(id), force),
        Command::Update {
            id,
            assignments,
            note,
            identity,
        } => commands::cmd_update(normalize_id(id), assignments, note, identity),
        Command::Dep { sub } => commands::cmd_dep(normalize_dep(sub)),
        Command::Link { sub } => commands::cmd_link(normalize_link(sub)),
        Command::Sync {
            remote,
            task,
            review,
            apply,
            discard,
            list_staged,
        } => commands::cmd_sync(commands::SyncArgs {
            remote,
            task,
            review,
            apply,
            discard,
            list_staged,
        }),
        Command::Resolve { file } => commands::cmd_resolve(file),
        Command::Prime { identity, json } => commands::cmd_prime(identity, json),
        Command::Repair {
            fix,
            forget_half_push,
            forget_all_half_pushes,
        } => commands::cmd_repair(fix, forget_half_push, forget_all_half_pushes),
        Command::Skill => {
            print!("{}", include_str!("../SKILL.md"));
            Ok(())
        }
        Command::Completions {
            shell,
            install,
            uninstall,
        } => handle_completions(shell, install, uninstall),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
