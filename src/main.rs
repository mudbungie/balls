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
        Command::Show { id, json } => commands::cmd_show(normalize_id(id), json),
        Command::Ready { json, no_fetch } => commands::cmd_ready(json, no_fetch),
        Command::Claim { id, identity, no_worktree } => commands::cmd_claim(normalize_id(id), identity, no_worktree),
        Command::Review { id, message } => commands::cmd_review(normalize_id(id), message),
        Command::Close { id, message } => commands::cmd_close(normalize_id(id), message),
        Command::Drop { id, force } => commands::cmd_drop(normalize_id(id), force),
        Command::Update {
            id,
            assignments,
            note,
            identity,
        } => commands::cmd_update(normalize_id(id), assignments, note, identity),
        Command::Dep { sub } => commands::cmd_dep(normalize_dep(sub)),
        Command::Link { sub } => commands::cmd_link(normalize_link(sub)),
        Command::Sync { remote, task } => commands::cmd_sync(remote, task),
        Command::Resolve { file } => commands::cmd_resolve(file),
        Command::Prime { identity, json } => commands::cmd_prime(identity, json),
        Command::Repair { fix } => commands::cmd_repair(fix),
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
mod tests {
    use super::*;

    #[test]
    fn bare_hex_gets_prefixed() {
        assert_eq!(normalize_id("534c".into()), "bl-534c");
        assert_eq!(normalize_id("abcdef0123".into()), "bl-abcdef0123");
    }

    #[test]
    fn already_prefixed_is_unchanged() {
        assert_eq!(normalize_id("bl-534c".into()), "bl-534c");
    }

    #[test]
    fn non_hex_is_unchanged() {
        assert_eq!(normalize_id("not-an-id".into()), "not-an-id");
        assert_eq!(normalize_id("xyz".into()), "xyz");
    }

    #[test]
    fn empty_is_unchanged() {
        assert_eq!(normalize_id(String::new()), "");
    }

    #[test]
    fn opt_and_vec_helpers_normalize_each() {
        assert_eq!(normalize_opt(Some("534c".into())), Some("bl-534c".into()));
        assert_eq!(normalize_opt(None), None);
        assert_eq!(
            normalize_vec(vec!["534c".into(), "bl-1e60".into()]),
            vec!["bl-534c".to_string(), "bl-1e60".to_string()]
        );
    }

    #[test]
    fn normalize_dep_add() {
        let DepCmd::Add { task, depends_on } = normalize_dep(DepCmd::Add {
            task: "534c".into(),
            depends_on: "1e60".into(),
        }) else {
            panic!("wrong variant");
        };
        assert_eq!(task, "bl-534c");
        assert_eq!(depends_on, "bl-1e60");
    }

    #[test]
    fn normalize_dep_rm() {
        let DepCmd::Rm { task, depends_on } = normalize_dep(DepCmd::Rm {
            task: "534c".into(),
            depends_on: "1e60".into(),
        }) else {
            panic!("wrong variant");
        };
        assert_eq!(task, "bl-534c");
        assert_eq!(depends_on, "bl-1e60");
    }

    #[test]
    fn normalize_dep_tree_some_and_none() {
        let DepCmd::Tree { id, json } = normalize_dep(DepCmd::Tree {
            id: Some("534c".into()),
            json: true,
        }) else {
            panic!("wrong variant");
        };
        assert_eq!(id, Some("bl-534c".into()));
        assert!(json);

        let DepCmd::Tree { id, json } = normalize_dep(DepCmd::Tree {
            id: None,
            json: false,
        }) else {
            panic!("wrong variant");
        };
        assert_eq!(id, None);
        assert!(!json);
    }

    #[test]
    fn normalize_link_add() {
        let LinkCmd::Add {
            task,
            link_type,
            target,
        } = normalize_link(LinkCmd::Add {
            task: "534c".into(),
            link_type: "relates_to".into(),
            target: "1e60".into(),
        })
        else {
            panic!("wrong variant");
        };
        assert_eq!(task, "bl-534c");
        assert_eq!(link_type, "relates_to");
        assert_eq!(target, "bl-1e60");
    }

    #[test]
    fn normalize_link_rm() {
        let LinkCmd::Rm {
            task,
            link_type,
            target,
        } = normalize_link(LinkCmd::Rm {
            task: "534c".into(),
            link_type: "relates_to".into(),
            target: "1e60".into(),
        })
        else {
            panic!("wrong variant");
        };
        assert_eq!(task, "bl-534c");
        assert_eq!(link_type, "relates_to");
        assert_eq!(target, "bl-1e60");
    }
}
