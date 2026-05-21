//! `bl plugin enable/disable/list` — CLI surface for plugin admin.
//! The mechanics live in `balls::plugin_admin`; this is arg handling
//! and human-facing output.

use super::discover;
use balls::error::Result;
use balls::plugin_admin::{self, DisableReport, EnableReport, Source};
use balls::plugin_policy::{self, PluginView, PolicyReport};
use serde_json::json;

pub fn cmd_plugin_enable(
    name: String,
    config_file: Option<String>,
    sync_on_change: bool,
) -> Result<()> {
    let store = discover()?;
    if sync_on_change {
        eprintln!(
            "warning: `--sync-on-change` is deprecated; set explicit SPEC §11 policy with \
             `bl plugin policy {name} <event>=<kind> ...`"
        );
    }
    let report = plugin_admin::enable(&store, &name, config_file, sync_on_change)?;
    print_enable(&name, &report);
    Ok(())
}

pub fn cmd_plugin_disable(name: String) -> Result<()> {
    let store = discover()?;
    let report = plugin_admin::disable(&store, &name)?;
    print_disable(&name, &report);
    Ok(())
}

pub fn cmd_plugin_list(json_mode: bool) -> Result<()> {
    let store = discover()?;
    let (plugins, source) = plugin_admin::load_effective(&store)?;
    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "source": source.as_str(),
                "plugins": plugins,
            }))?
        );
        return Ok(());
    }
    if plugins.is_empty() {
        println!("no plugins enabled (source: {})", source.as_str());
        return Ok(());
    }
    println!("source: {}", source.as_str());
    for (name, entry) in &plugins {
        let on = if entry.enabled { "on" } else { "off" };
        let sync = if entry.sync_on_change { "+sync" } else { "" };
        let part = entry
            .participant
            .as_ref()
            .map_or(0, |p| p.subscriptions.len());
        let part = if part > 0 {
            format!(" participant={part}-events")
        } else {
            String::new()
        };
        println!(
            "  {name} [{on}{sync}] {file}{part}",
            file = entry.config_file
        );
    }
    Ok(())
}

fn print_enable(name: &str, r: &EnableReport) {
    let where_ = match r.source {
        Source::Hub => "hub (state-repo, balls/tasks)",
        Source::Project => "project",
    };
    println!("enabled {name} on {where_}");
    if r.file_created {
        println!("  created {}", r.file_path.display());
    } else {
        println!("  using existing {}", r.file_path.display());
    }
    follow_up_hint(r.source, &[".balls/config.json"]);
}

fn print_disable(name: &str, r: &DisableReport) {
    let where_ = match r.source {
        Source::Hub => "hub (state-repo, balls/tasks)",
        Source::Project => "project",
    };
    println!("disabled {name} on {where_} (config file kept)");
    follow_up_hint(r.source, &[".balls/config.json"]);
}

fn follow_up_hint(source: Source, paths: &[&str]) {
    match source {
        Source::Hub => {
            println!("  run `bl sync` to publish to the hub");
        }
        Source::Project => {
            let joined = paths.join(" ");
            println!("  commit to publish: git add {joined} && git commit");
        }
    }
}

pub fn cmd_plugin_policy(
    name: String,
    set: Vec<String>,
    rm: Vec<String>,
    clear: bool,
    no_legacy: bool,
) -> Result<()> {
    let store = discover()?;
    let op = plugin_policy::parse_op(&set, &rm, clear, no_legacy)?;
    let report = plugin_policy::apply(&store, &name, op)?;
    print_policy(&name, &set, &rm, clear, no_legacy, &report);
    Ok(())
}

fn print_policy(
    name: &str,
    set: &[String],
    rm: &[String],
    clear: bool,
    no_legacy: bool,
    r: &PolicyReport,
) {
    let where_ = r.source.as_str();
    if clear {
        println!("cleared participant block for {name} ({where_})");
        println!("  {name} now falls back to the legacy sync_on_change mapping");
    } else if no_legacy {
        println!("set {name} to explicit empty subscriptions ({where_})");
        println!("  legacy fallback suppressed — {name} participates in no events");
    } else if !rm.is_empty() {
        println!(
            "dropped {n} subscription(s) for {name} ({where_}): {events}",
            n = rm.len(),
            events = rm.join(", ")
        );
    } else {
        println!(
            "updated participant policy for {name} ({where_}): {tokens}",
            tokens = set.join(", ")
        );
    }
    follow_up_hint(r.source, &[".balls/config.json"]);
}

pub fn cmd_plugin_show(name: String, json_mode: bool) -> Result<()> {
    let store = discover()?;
    let view = plugin_policy::describe(&store, &name)?;
    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "name": name,
                "source": view.source.as_str(),
                "explicit": view.explicit,
                "entry": &view.entry,
                "resolved": &view.resolved,
            }))?
        );
        return Ok(());
    }
    print_show(&name, &view);
    Ok(())
}

fn print_show(name: &str, v: &PluginView) {
    println!("plugin {name} (source: {})", v.source.as_str());
    println!("  enabled:        {}", v.entry.enabled);
    println!("  sync_on_change: {}", v.entry.sync_on_change);
    println!("  config_file:    {}", v.entry.config_file);
    let subs = &v.resolved.subscriptions;
    if !v.explicit {
        println!("  participant:    legacy (resolved from sync_on_change)");
    } else if subs.is_empty() {
        println!("  participant:    explicit, no subscriptions (plugin is silent)");
    } else {
        println!("  participant:    explicit ({}-events)", subs.len());
    }
    if subs.is_empty() {
        println!("  resolved policy: (none)");
    } else {
        println!("  resolved policy:");
        for (ev, ep) in subs {
            let ev_name = plugin_policy::event_name(*ev);
            let kind = plugin_policy::kind_name(ep.policy);
            println!("    {ev_name:<8} {kind}");
        }
    }
}
