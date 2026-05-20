//! The relations block of `bl show`: deps (with inline statuses),
//! gates, other links, parent, children + completion, delivered,
//! branch, repo, and external-remote rows.
//!
//! Split out of `render_show` so that module stays scoped to the
//! header/meta/description/notes layout. `write_relations` is the
//! only entry point — the per-section writers are private because
//! nothing but the orchestrator sequences them.

use crate::delivery::{self, Delivery};
use crate::ready;
use crate::render_show::Ctx;
use crate::sanitize;
use crate::task::{LinkType, Task};
use std::fmt::Write as _;
use std::path::Path;

pub(crate) fn write_relations(
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
        || t.repo.is_some()
        || t.delivered_repo.is_some()
        || t.target_branch.is_some()
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
    write_delivered_repo(out, t);
    write_branch(out, t);
    write_target_branch(out, t);
    write_repo(out, t);
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
        .map_or(String::new(), |p| format!("  {}", sanitize::inline(&p.title)));
    let _ = writeln!(out, "  parent:   {pid}{title}");
}

fn write_children(out: &mut String, t: &Task, all: &[Task]) {
    let kids = ready::children_of(all, &t.id);
    if kids.is_empty() && t.closed_children.is_empty() {
        return;
    }
    let _ = writeln!(out, "  children:");
    for k in &kids {
        let s = sanitize::inline(&k.title);
        let _ = writeln!(out, "    {} [{}] {}", k.id, k.status.as_str(), s);
    }
    for a in &t.closed_children {
        let s = sanitize::inline(&a.title);
        let _ = writeln!(out, "    {} [archived] {}", a.id, s);
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
        let _ = writeln!(out, "  branch:   {}", sanitize::inline(b));
    }
}

fn write_repo(out: &mut String, t: &Task) {
    if let Some(r) = &t.repo {
        let _ = writeln!(out, "  repo:     {}", sanitize::inline(r));
    }
}

fn write_delivered_repo(out: &mut String, t: &Task) {
    if let Some(r) = &t.delivered_repo {
        let _ = writeln!(out, "  delivered repo: {}", sanitize::inline(r));
    }
}

fn write_target_branch(out: &mut String, t: &Task) {
    if let Some(b) = &t.target_branch {
        let _ = writeln!(out, "  target:   {}", sanitize::inline(b));
    }
}

fn write_external(out: &mut String, t: &Task) {
    let rows: Vec<(String, Option<String>, Option<String>)> = t
        .external
        .iter()
        .filter_map(|(name, value)| {
            let obj = value.as_object()?;
            let key = obj.get("remote_key").and_then(|v| v.as_str()).map(sanitize::inline);
            let url = obj.get("remote_url").and_then(|v| v.as_str()).map(sanitize::inline);
            if key.is_none() && url.is_none() {
                return None;
            }
            Some((sanitize::inline(name), key, url))
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

#[cfg(test)]
#[path = "render_show_relations_tests.rs"]
mod tests;
