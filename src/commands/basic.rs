//! init, create, list, show, ready — the read-mostly commands.

use super::discover;
use super::id_gen::generate_unique_id;
use balls::error::{BallError, Result};
use balls::git;
use balls::plugin;
use balls::ready;
use balls::store::{task_lock, Store};
use balls::task::{NewTaskOpts, Status, Task, TaskType};
use std::env;
use std::fs;

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
        for t in &tasks {
            println!(
                "[P{}] {} {:<12} {}",
                t.priority,
                t.id,
                t.status.as_str(),
                t.title
            );
        }
    }
    Ok(())
}

pub fn cmd_show(id: String, json: bool) -> Result<()> {
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

    render_text(&task, &all, &id, &delivery, &store.root);
    Ok(())
}

fn render_text(
    task: &Task,
    all: &[Task],
    id: &str,
    delivery: &balls::delivery::Delivery,
    repo_root: &std::path::Path,
) {
    println!("{} - {}", task.id, task.title);
    println!("  type:     {:?}", task.task_type);
    println!("  priority: {}", task.priority);
    println!("  status:   {}", task.status.as_str());
    if let Some(p) = &task.parent {
        println!("  parent:   {p}");
    }
    if !task.depends_on.is_empty() {
        println!("  deps:     {}", task.depends_on.join(", "));
    }
    if !task.links.is_empty() {
        println!("  links:");
        for l in &task.links {
            println!("    {} {}", l.link_type.as_str(), l.target);
        }
    }
    if !task.tags.is_empty() {
        println!("  tags:     {}", task.tags.join(", "));
    }
    if let Some(c) = &task.claimed_by {
        println!("  claimed:  {c}");
    }
    if let Some(b) = &task.branch {
        println!("  branch:   {b}");
    }
    if let Some(sha) = &delivery.sha {
        let label = if delivery.hint_stale { " (hint stale)" } else { "" };
        println!(
            "  delivered: {}{}",
            balls::delivery::describe(repo_root, sha),
            label
        );
    }
    if ready::is_dep_blocked(all, task) {
        println!("  dep_blocked: yes");
    }
    let kids = ready::children_of(all, id);
    if !kids.is_empty() || !task.closed_children.is_empty() {
        println!("  children:");
        for k in &kids {
            println!("    {} [{}] {}", k.id, k.status.as_str(), k.title);
        }
        for a in &task.closed_children {
            println!("    {} [archived] {}", a.id, a.title);
        }
        println!("  completion: {:.0}%", ready::completion(all, id) * 100.0);
    }
    if !task.description.is_empty() {
        println!();
        println!("{}", task.description);
    }
    if !task.notes.is_empty() {
        println!();
        println!("notes:");
        for n in &task.notes {
            println!("  [{}] {}: {}", n.ts.to_rfc3339(), n.author, n.text);
        }
    }
}

pub fn cmd_ready(json: bool, no_fetch: bool) -> Result<()> {
    let store = discover()?;
    let cfg = store.load_config()?;

    if cfg.auto_fetch_on_ready && !no_fetch {
        maybe_auto_fetch(&store, cfg.stale_threshold_seconds);
    }

    let tasks = store.all_tasks()?;
    let ready = ready::ready_queue(&tasks);
    if json {
        let v: Vec<&Task> = ready;
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else if ready.is_empty() {
        println!("No tasks ready.");
    } else {
        for t in &ready {
            println!("[P{}] {} {}", t.priority, t.id, t.title);
        }
    }
    Ok(())
}

fn maybe_auto_fetch(store: &Store, stale_threshold_seconds: u64) {
    let last_fetch = store.local_dir().join("last_fetch");
    let stale = match fs::metadata(&last_fetch).and_then(|m| m.modified()) {
        Ok(t) => std::time::SystemTime::now()
            .duration_since(t)
            .map(|d| d.as_secs() > stale_threshold_seconds)
            .unwrap_or(true),
        Err(_) => true,
    };
    if stale && git::git_has_remote(&store.root, "origin") {
        let _ = git::git_fetch(&store.root, "origin");
        let _ = fs::write(&last_fetch, "");
    }
}
