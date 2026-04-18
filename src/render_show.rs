//! Rich rendering for `bl show`.
//!
//! Lays out a header line (priority + status + id + title + claimed
//! badge), a compact metadata row (type, relative timestamps, tags),
//! a relations block (deps with inline statuses, gates, parent,
//! children, delivered/branch/remote meta), a wrapped description,
//! and an oldest-first notes log. Relative timestamps use a `now`
//! passed in so the whole module is pure and testable.

use crate::delivery::{self, Delivery};
use crate::display::Display;
use crate::progress;
use crate::ready;
use crate::render_show_text::{format_time, wrap};
use crate::task::{LinkType, Task, TaskType};
use chrono::{DateTime, Utc};
use std::fmt::Write as _;
use std::path::Path;

pub struct Ctx<'a> {
    pub d: Display,
    pub me: &'a str,
    pub columns: usize,
    pub verbose: bool,
    pub now: DateTime<Utc>,
}

pub fn render(
    task: &Task,
    all: &[Task],
    delivery: &Delivery,
    repo_root: &Path,
    ctx: &Ctx<'_>,
) -> String {
    let mut out = String::new();
    write_header(&mut out, task, ctx);
    write_meta_row(&mut out, task, ctx);
    write_tags(&mut out, task);
    write_progress(&mut out, task, all, ctx);
    write_relations(&mut out, task, all, delivery, repo_root, ctx);
    write_description(&mut out, task, ctx.columns);
    write_notes(&mut out, task, ctx);
    out
}

fn write_progress(out: &mut String, t: &Task, all: &[Task], ctx: &Ctx<'_>) {
    if !matches!(t.task_type, TaskType::Epic) {
        return;
    }
    let (closed, total) = progress::counts(all, &t.id);
    let _ = writeln!(out, "  progress: {}", progress::summary(closed, total, ctx.d));
}

fn write_header(out: &mut String, t: &Task, ctx: &Ctx<'_>) {
    let claimed = match &t.claimed_by {
        Some(c) if !c.is_empty() => format!(
            "  {} claimed by {c}",
            if ctx.d.use_unicode() { "★" } else { "*" },
        ),
        _ => String::new(),
    };
    let _ = writeln!(
        out,
        "{} {} {} {}  {}{claimed}",
        ctx.d.prio_dot(t.priority),
        ctx.d.status_glyph(&t.status),
        ctx.d.status_word(&t.status),
        t.id,
        t.title,
    );
}

fn write_meta_row(out: &mut String, t: &Task, ctx: &Ctx<'_>) {
    let created = format_time(t.created_at, ctx);
    let updated = format_time(t.updated_at, ctx);
    let type_name = match &t.task_type {
        TaskType::Epic => "epic",
        TaskType::Task => "task",
        TaskType::Bug => "bug",
        TaskType::Unknown(s) => s.as_str(),
    };
    let _ = writeln!(
        out,
        "  type: {type_name}      created: {created}       updated: {updated}",
    );
}

fn write_tags(out: &mut String, t: &Task) {
    if t.tags.is_empty() {
        return;
    }
    let _ = writeln!(out, "  tags: {}", t.tags.join(", "));
}

fn write_relations(
    out: &mut String,
    t: &Task,
    all: &[Task],
    delivery: &Delivery,
    repo_root: &Path,
    ctx: &Ctx<'_>,
) {
    let has_any = !t.depends_on.is_empty()
        || !t.links.is_empty()
        || t.parent.is_some()
        || !ready::children_of(all, &t.id).is_empty()
        || !t.closed_children.is_empty()
        || delivery.sha.is_some()
        || t.branch.is_some()
        || !t.external.is_empty();
    if !has_any {
        return;
    }
    out.push('\n');
    write_deps(out, t, all, ctx);
    write_gates(out, t, all, ctx);
    write_other_links(out, t);
    write_parent(out, t, all);
    write_children(out, t, all);
    write_delivered(out, delivery, repo_root);
    write_branch(out, t);
    write_external(out, t);
    if ready::is_dep_blocked(all, t) {
        let _ = writeln!(out, "  dep_blocked: yes");
    }
}

fn write_other_links(out: &mut String, t: &Task) {
    let others: Vec<&crate::link::Link> = t
        .links
        .iter()
        .filter(|l| !matches!(l.link_type, LinkType::Gates))
        .collect();
    if others.is_empty() {
        return;
    }
    let _ = writeln!(out, "  links:");
    for l in others {
        let _ = writeln!(out, "    {} {}", l.link_type.as_str(), l.target);
    }
}

fn write_deps(out: &mut String, t: &Task, all: &[Task], ctx: &Ctx<'_>) {
    if t.depends_on.is_empty() {
        return;
    }
    let rendered: Vec<String> = t
        .depends_on
        .iter()
        .map(|id| match all.iter().find(|o| &o.id == id) {
            Some(o) => format!(
                "{} {} {}",
                id,
                ctx.d.status_glyph(&o.status),
                o.status.as_str(),
            ),
            None => format!("{id} (archived)"),
        })
        .collect();
    let _ = writeln!(out, "  deps:     {}", rendered.join("   "));
}

fn write_gates(out: &mut String, t: &Task, all: &[Task], ctx: &Ctx<'_>) {
    let gates: Vec<&str> = t
        .links
        .iter()
        .filter_map(|l| match l.link_type {
            LinkType::Gates => Some(l.target.as_str()),
            _ => None,
        })
        .collect();
    if gates.is_empty() {
        return;
    }
    let rendered: Vec<String> = gates
        .into_iter()
        .map(|id| match all.iter().find(|o| o.id == id) {
            Some(o) => format!(
                "{} {} {}",
                id,
                ctx.d.status_glyph(&o.status),
                o.status.as_str(),
            ),
            None => format!("{id} (closed)"),
        })
        .collect();
    let _ = writeln!(out, "  gates:    {}", rendered.join("   "));
}

fn write_parent(out: &mut String, t: &Task, all: &[Task]) {
    let Some(pid) = &t.parent else { return };
    let title = all
        .iter()
        .find(|p| &p.id == pid)
        .map_or(String::new(), |p| format!("  {}", p.title));
    let _ = writeln!(out, "  parent:   {pid}{title}");
}

fn write_children(out: &mut String, t: &Task, all: &[Task]) {
    let kids = ready::children_of(all, &t.id);
    if kids.is_empty() && t.closed_children.is_empty() {
        return;
    }
    let _ = writeln!(out, "  children:");
    for k in &kids {
        let _ = writeln!(out, "    {} [{}] {}", k.id, k.status.as_str(), k.title);
    }
    for a in &t.closed_children {
        let _ = writeln!(out, "    {} [archived] {}", a.id, a.title);
    }
    let _ = writeln!(
        out,
        "  completion: {:.0}%",
        ready::completion(all, &t.id) * 100.0,
    );
}

fn write_delivered(out: &mut String, delivery: &Delivery, repo_root: &Path) {
    let Some(sha) = &delivery.sha else { return };
    let label = if delivery.hint_stale { " (hint stale)" } else { "" };
    let _ = writeln!(
        out,
        "  delivered: {}{}",
        delivery::describe(repo_root, sha),
        label,
    );
}

fn write_branch(out: &mut String, t: &Task) {
    if let Some(b) = &t.branch {
        let _ = writeln!(out, "  branch:   {b}");
    }
}

fn write_external(out: &mut String, t: &Task) {
    let rows: Vec<(String, Option<String>, Option<String>)> = t
        .external
        .iter()
        .filter_map(|(name, value)| {
            let obj = value.as_object()?;
            let key = obj.get("remote_key").and_then(|v| v.as_str()).map(String::from);
            let url = obj.get("remote_url").and_then(|v| v.as_str()).map(String::from);
            if key.is_none() && url.is_none() {
                return None;
            }
            Some((name.clone(), key, url))
        })
        .collect();
    if rows.is_empty() {
        return;
    }
    let _ = writeln!(out, "  remote:");
    for (name, key, url) in rows {
        let parts: Vec<String> = [key, url].into_iter().flatten().collect();
        let _ = writeln!(out, "    {name}: {}", parts.join(" "));
    }
}

fn write_description(out: &mut String, t: &Task, columns: usize) {
    if t.description.is_empty() {
        return;
    }
    out.push('\n');
    out.push_str("  description\n");
    for line in wrap(&t.description, columns.saturating_sub(4).max(1)) {
        let _ = writeln!(out, "    {line}");
    }
}

fn write_notes(out: &mut String, t: &Task, ctx: &Ctx<'_>) {
    if t.notes.is_empty() {
        return;
    }
    out.push('\n');
    let _ = writeln!(out, "  notes ({})", t.notes.len());
    for n in &t.notes {
        let when = format_time(n.ts, ctx);
        let _ = writeln!(out, "    {when}  {}  — {}", n.author, n.text);
    }
}

#[cfg(test)]
#[path = "render_show_tests.rs"]
mod tests;
