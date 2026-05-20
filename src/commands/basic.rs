//! init, create, list, show, ready — the read-mostly commands.

use super::discover;
use balls::display;
use balls::error::{BallError, Result};
use balls::participant::Event;
use balls::participant_config::{override_tokens, InvocationOverrides};
use balls::plugin::{self, Rollback};
use balls::ready;
use balls::render_list;
use balls::store::{task_lock, Store};
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
    task.repo = Some(repo_identity(&store));
    task.target_branch = target_branch;
    let rb = plugin::state_head(&store)?;
    {
        let _g = task_lock(&store, &id)?;
        store.save_task(&task)?;
        store.commit_task(&id, &format!("balls: create {id} - {title}"))?;
    }

    // SPEC §6.1: task birth is its own event, not an `Update`. A
    // mirror-on-create plugin can finally tell creation from change.
    // No pre-image — `create` has no prior (SPEC §5.1, correctly
    // absent). A required veto rewinds the create commit (§9).
    let tokens = override_tokens(&overrides, false, false);
    plugin::finish(
        &store,
        None,
        &task,
        Event::Create,
        &super::default_identity(),
        &overrides,
        &tokens,
        Rollback::State(rb.as_deref()),
    )?;

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

pub fn cmd_show(id: String, json: bool, verbose: bool) -> Result<()> {
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
        .map_or(
            balls::delivery::Delivery {
                sha: None,
                hint_stale: false,
            },
            |b| balls::delivery::resolve(&store.root, &b, &task),
        );

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
