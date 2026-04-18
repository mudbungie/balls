//! init, create, list, show, ready — the read-mostly commands.

use super::discover;
use super::id_gen::generate_unique_id;
use balls::display;
use balls::error::{BallError, Result};
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

    let task = Task::new(opts, id.clone());
    {
        let _g = task_lock(&store, &id)?;
        store.save_task(&task)?;
        store.commit_task(&id, &format!("balls: create {id} - {title}"))?;
    }

    if let Ok(results) = plugin::run_plugin_push(&store, &task) {
        let _ = plugin::apply_push_response(&store, &id, &results);
    }

    println!("{id}");
    Ok(())
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
        let pretty = serde_json::json!({
            "task": task,
            "dep_blocked": blocked,
            "children": children.iter().map(|t| &t.id).collect::<Vec<_>>(),
            "closed_children": task.closed_children,
            "completion": ready::completion(&all, &id),
            "delivered_in_resolved": delivery.sha,
            "delivered_in_hint_stale": delivery.hint_stale,
        });
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

