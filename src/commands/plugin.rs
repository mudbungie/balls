//! `bl plugin enable/disable/list` — CLI surface for plugin admin.
//! The mechanics live in `balls::plugin_admin`; this is arg handling
//! and human-facing output.

use super::discover;
use balls::error::Result;
use balls::plugin_admin::{self, DisableReport, EnableReport, Source};
use serde_json::json;

pub fn cmd_plugin_enable(
    name: String,
    config_file: Option<String>,
    sync_on_change: bool,
) -> Result<()> {
    let store = discover()?;
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
