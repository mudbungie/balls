//! init, create, list, show, ready — the read-mostly commands.

use super::discover;
use super::plumbing::finish_state_event;
use balls::display;
use balls::error::{BallError, Result};
use balls::participant::Event;
use balls::participant_config::InvocationOverrides;
use balls::ready;
use balls::render_list;
use balls::store::task_lock;
use balls::task::{NewTaskOpts, Status, Task, TaskType};
use std::env;

/// CLI inputs for `bl create`, bundled so the function stays under
/// clippy's argument cap — mirrors `SyncArgs`. `main.rs` threads the
/// clap flags through one struct.
pub struct CreateArgs {
    pub title: String,
    pub priority: u8,
    pub task_type: String,
    pub parent: Option<String>,
    pub dep: Vec<String>,
    pub tag: Vec<String>,
    pub description: String,
    pub target_branch: Option<String>,
    pub overrides: InvocationOverrides,
}

pub fn cmd_create(args: CreateArgs) -> Result<()> {
    let CreateArgs {
        title,
        priority,
        task_type,
        parent,
        dep,
        tag,
        description,
        target_branch,
        overrides,
    } = args;
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

    let id = balls::task_id::generate_task_id(&store, &title)?;
    // New-task cycle check is unnecessary: a fresh id has no dependants yet,
    // so no chain through `dep` can reach it. Existing deps were already
    // validated above.

    let mut task = Task::new(opts, id.clone());
    // Only a fetchable `origin` URL — never a bare basename. Create
    // anchors on where the ball was *filed*, which on a bare hub or a
    // forge-sync bridge is not the code repo; `bl claim` re-anchors
    // `repo` to the authoritative code home (bl-8994).
    task.repo = balls::repo_url::origin_url(&store.root);
    task.target_branch = target_branch;

    // SPEC §6.1: task birth is its own event, not an `Update`. A
    // mirror-on-create plugin can finally tell creation from change.
    // No pre-image — `create` has no prior (SPEC §5.1, correctly
    // absent). A required veto rewinds the create commit (§9).
    finish_state_event(
        &store,
        Event::Create,
        &super::default_identity(),
        &overrides,
        false,
        false,
        || {
            let _g = task_lock(&store, &id)?;
            store.save_task(&task)?;
            store.commit_task(&id, &format!("balls: create {id} - {title}"))?;
            Ok((None, task))
        },
    )?;

    println!("{id}");
    Ok(())
}

pub fn cmd_list(
    status: Option<String>,
    priority: Option<u8>,
    parent: Option<String>,
    tag: Option<String>,
    all: bool,
    closed: bool,
    json: bool,
) -> Result<()> {
    let store = discover()?;
    let st = status.as_deref().map(Status::parse).transpose()?;
    let want_closed = closed || st.as_ref() == Some(&Status::Closed);

    if (want_closed || all) && !balls::archive_recovery::available(&store) {
        eprintln!(
            "note: closed tasks live only in the git state branch; \
             unavailable in this store"
        );
    }

    // `--status X` (X != closed) keeps its historical precedence over
    // `--all`. `--closed`/`--status closed` reconstruct from history;
    // `--all` folds that history in alongside the live set.
    let mut tasks = if want_closed {
        balls::archive_recovery::recover_all(&store)
    } else if let Some(s) = &st {
        let mut live = store.all_tasks()?;
        live.retain(|t| &t.status == s);
        live
    } else {
        let mut live = store.all_tasks()?;
        if all {
            live.extend(balls::archive_recovery::recover_all(&store));
        } else {
            live.retain(|t| t.status != Status::Closed);
        }
        live
    };

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
        // Grouped view (GROUP_ORDER) deliberately drops Closed, so any
        // set that may carry closed tasks renders flat.
        let flat = want_closed || all || st.is_some();
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

pub fn cmd_show(id: String, json: bool, verbose: bool, resolve_remote: bool) -> Result<()> {
    let store = discover()?;
    // A closed task's file is gone from the state-branch HEAD; fall
    // back to reconstructing it from history so `bl show <id>` keeps
    // the promise its own help text makes for closed tasks.
    let task = match store.load_task(&id) {
        Err(BallError::TaskNotFound(_)) => balls::archive_recovery::recover_one(&store, &id)
            .ok_or(BallError::TaskNotFound(id.clone()))?,
        other => other?,
    };
    let all = store.all_tasks()?;
    // Resolve the integration branch through the single seam; if the
    // store can't answer (no-git, branch missing) there's nothing to
    // resolve a delivery against, so fall back to an empty result.
    let delivery = store
        .load_config()
        .and_then(|c| c.integration_branch_for(&store.root, task.target_branch.as_deref()))
        .map_or(balls::delivery::Delivery::default(), |b| {
            balls::delivery::resolve_with(
                &store.root,
                &b,
                &task,
                balls::delivery::ResolveOpts { remote: resolve_remote },
            )
        });

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
            "delivered_in_resolved_repo": delivery.resolved_repo,
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

