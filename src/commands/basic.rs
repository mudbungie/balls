//! init, create, list, show, ready — the read-mostly commands.

use super::discover;
use super::id_gen::generate_unique_id;
use balls::display;
use balls::error::{BallError, Result};
use balls::participant::Event;
use balls::plugin;
use balls::ready;
use balls::render_list;
use balls::store::{task_lock, Store};
use balls::task::{NewTaskOpts, Status, Task, TaskType};
use std::env;

pub fn cmd_init(stealth: bool, tasks_dir: Option<String>) -> Result<()> {
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

pub fn cmd_create(
    title: String,
    priority: u8,
    task_type: String,
    parent: Option<String>,
    dep: Vec<String>,
    tag: Vec<String>,
    description: String,
) -> Result<()> {
    let store = discover()?;

    balls::task::validate_priority(priority)?;
    let task_type = TaskType::parse(&task_type)?;

    let all = store.all_tasks()?;
    if let Some(pid) = &parent {
        if !all.iter().any(|t| &t.id == pid) {
            return Err(BallError::InvalidTask(format!("parent not found: {pid}")));
        }
    }
    ready::validate_deps(&all, &dep)?;

    let opts = NewTaskOpts {
        title: title.clone(),
        task_type,
        priority,
        parent,
        depends_on: dep.clone(),
        description,
        tags: tag,
    };

    let cfg = store.load_config()?;
    let id = generate_unique_id(&title, &store, cfg.id_length)?;
    // New-task cycle check is unnecessary: a fresh id has no dependants yet,
    // so no chain through `dep` can reach it. Existing deps were already
    // validated above.

    let mut task = Task::new(opts, id.clone());
    task.repo = Some(repo_identity(&store));
    {
        let _g = task_lock(&store, &id)?;
        store.save_task(&task)?;
        store.commit_task(&id, &format!("balls: create {id} - {title}"))?;
    }

    let _ = plugin::dispatch_push(&store, &task, Event::Update, &super::default_identity());

    println!("{id}");
    Ok(())
}

/// Provenance string for tasks created here: the code repo's `origin`
/// URL when there is one (stable across clones — the right key once
/// many repos share a hub), otherwise the repo path.
fn repo_identity(store: &Store) -> String {
    identity_from(git_origin_url(&store.root), &store.root)
}

fn git_origin_url(root: &std::path::Path) -> Option<String> {
    let out = balls::git::clean_git_command(root)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!url.is_empty()).then_some(url)
}

/// Pure choice: `origin` URL if present, else the repo's directory
/// name, else the full path. Split out so every branch is unit-tested
/// without spawning git.
fn identity_from(url: Option<String>, root: &std::path::Path) -> String {
    url.or_else(|| root.file_name().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| root.to_string_lossy().into_owned())
}

pub fn cmd_list(
    status: Option<String>,
    priority: Option<u8>,
    parent: Option<String>,
    tag: Option<String>,
    all: bool,
    json: bool,
) -> Result<()> {
    let store = discover()?;
    let mut tasks = store.all_tasks()?;

    if let Some(s) = &status {
        let st = Status::parse(s)?;
        tasks.retain(|t| t.status == st);
    } else if !all {
        tasks.retain(|t| t.status != Status::Closed);
    }
    if let Some(p) = priority {
        tasks.retain(|t| t.priority == p);
    }
    if let Some(pid) = &parent {
        tasks.retain(|t| t.parent.as_deref() == Some(pid.as_str()));
    }
    if let Some(tg) = &tag {
        tasks.retain(|t| t.tags.iter().any(|x| x == tg));
    }
    tasks.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
    });

    if json {
        println!("{}", serde_json::to_string_pretty(&tasks)?);
    } else {
        let flat = status.is_some();
        let me = super::default_identity();
        let cols = terminal_columns();
        let all = store.all_tasks()?;
        let ctx = render_list::Ctx {
            d: display::global(),
            me: &me,
            columns: cols,
            all: &all,
        };
        print!("{}", render_list::render(&tasks, flat, &ctx));
    }
    Ok(())
}

fn terminal_columns() -> usize {
    env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100)
}

pub fn cmd_show(id: String, json: bool, verbose: bool) -> Result<()> {
    let store = discover()?;
    let task = store.load_task(&id)?;
    let all = store.all_tasks()?;
    let delivery = balls::delivery::resolve(&store.root, &task);

    if json {
        let blocked = ready::is_dep_blocked(&all, &task);
        let children: Vec<&Task> = ready::children_of(&all, &id);
        let mut pretty = serde_json::json!({
            "task": task,
            "dep_blocked": blocked,
            "children": children.iter().map(|t| &t.id).collect::<Vec<_>>(),
            "closed_children": task.closed_children,
            "completion": ready::completion(&all, &id),
            "delivered_in_resolved": delivery.sha,
            "delivered_in_hint_stale": delivery.hint_stale,
        });
        if task.task_type.is_epic() {
            let (closed, total) = balls::progress::counts(&all, &id);
            pretty["progress"] =
                serde_json::json!({ "closed": closed, "total": total });
        }
        println!("{}", serde_json::to_string_pretty(&pretty)?);
        return Ok(());
    }

    let me = super::default_identity();
    let ctx = balls::render_show::Ctx {
        d: display::global(),
        me: &me,
        columns: terminal_columns(),
        verbose,
        now: chrono::Utc::now(),
    };
    print!(
        "{}",
        balls::render_show::render(&task, &all, &delivery, &store.root, &ctx),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::identity_from;
    use std::path::Path;

    #[test]
    fn identity_prefers_url() {
        let got = identity_from(Some("git@h:proj.git".into()), Path::new("/x/y"));
        assert_eq!(got, "git@h:proj.git");
    }

    #[test]
    fn identity_falls_back_to_basename() {
        assert_eq!(identity_from(None, Path::new("/x/myrepo")), "myrepo");
    }

    #[test]
    fn identity_falls_back_to_path_when_no_basename() {
        assert_eq!(identity_from(None, Path::new("/")), "/");
    }
}
