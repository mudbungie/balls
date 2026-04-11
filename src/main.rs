mod cli;
mod commands;

use clap::Parser;
use cli::{Cli, Command};

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Init { stealth } => commands::cmd_init(stealth),
        Command::Create {
            title,
            priority,
            task_type,
            parent,
            dep,
            tag,
            description,
        } => commands::cmd_create(title, priority, task_type, parent, dep, tag, description),
        Command::List {
            status,
            priority,
            parent,
            tag,
            all,
            json,
        } => commands::cmd_list(status, priority, parent, tag, all, json),
        Command::Show { id, json } => commands::cmd_show(id, json),
        Command::Ready { json, no_fetch } => commands::cmd_ready(json, no_fetch),
        Command::Claim { id, identity } => commands::cmd_claim(id, identity),
        Command::Review { id, message } => commands::cmd_review(id, message),
        Command::Close { id, message } => commands::cmd_close(id, message),
        Command::Drop { id, force } => commands::cmd_drop(id, force),
        Command::Update {
            id,
            assignments,
            note,
            identity,
        } => commands::cmd_update(id, assignments, note, identity),
        Command::Dep { sub } => commands::cmd_dep(sub),
        Command::Link { sub } => commands::cmd_link(sub),
        Command::Sync { remote, task } => commands::cmd_sync(remote, task),
        Command::Resolve { file } => commands::cmd_resolve(file),
        Command::Prime { identity, json } => commands::cmd_prime(identity, json),
        Command::Repair { fix } => commands::cmd_repair(fix),
        Command::Skill => {
            print!("{}", include_str!("../SKILL.md"));
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
