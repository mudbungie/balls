use super::native_types::{DescribeResponse, ProposeResponse};
use super::types::{PushResponse, SyncReport};
use crate::config::PluginEntry;
use crate::error::Result;
use crate::participant::Event;
use crate::store::Store;
use crate::task::Task;
use std::path::PathBuf;

// Subprocess plumbing (spawn_and_run / command / parse_outcome /
// render_diagnostics) is a second `impl Plugin` block in
// `runner_proc` — kept apart so this file is just the plugin
// operation surface.

pub struct Plugin {
    pub executable: String,
    pub config_path: PathBuf,
    pub auth_dir: PathBuf,
}

impl Plugin {
    pub fn resolve(store: &Store, name: &str, entry: &PluginEntry) -> Self {
        let executable = format!("balls-plugin-{name}");
        let config_path = store.root.join(&entry.config_file);
        let auth_dir = store.local_plugins_dir().join(name);
        Plugin {
            executable,
            config_path,
            auth_dir,
        }
    }

    fn is_available(&self) -> bool {
        which(&self.executable).is_some()
    }

    /// Check if the plugin's auth is valid. Returns true if auth is good,
    /// false if expired/missing. Plugin diagnostics (if any) are
    /// rendered before the warning, so a plugin that explains *why* auth
    /// is missing via the diagnostics channel gets its message surfaced.
    pub fn auth_check(&self) -> bool {
        if !self.is_available() {
            return false;
        }
        let Ok(outcome) = self.spawn_and_run("auth-check", &[], &[]) else {
            return false;
        };
        self.render_diagnostics("auth-check", &outcome.diagnostics);
        if outcome.status.success() {
            true
        } else {
            eprintln!(
                "warning: {} auth expired. Run `{} auth-setup --config {} --auth-dir {}` to re-authenticate.",
                self.executable,
                self.executable,
                self.config_path.display(),
                self.auth_dir.display()
            );
            false
        }
    }

    /// Run the plugin's push command. Returns the plugin's response
    /// (stored into task.external) or None if the plugin failed,
    /// timed out, or exceeded the output cap.
    pub fn push(&self, task: &Task) -> Result<Option<PushResponse>> {
        let json = serde_json::to_string(task)?;
        let outcome =
            self.spawn_and_run("push", &[("--task", &task.id)], json.as_bytes())?;
        Ok(self.parse_outcome::<PushResponse>("push", outcome))
    }

    /// Run the plugin's sync command. Sends all local tasks on stdin.
    pub fn sync(
        &self,
        tasks: &[Task],
        filter: Option<&str>,
    ) -> Result<Option<SyncReport>> {
        let json = serde_json::to_string(tasks)?;
        let extra: &[(&str, &str)] = match filter {
            Some(id) => &[("--task", id)],
            None => &[],
        };
        let outcome = self.spawn_and_run("sync", extra, json.as_bytes())?;
        Ok(self.parse_outcome::<SyncReport>("sync", outcome))
    }

    /// Native protocol §5.2 (bl-8b71): plugin self-describes its
    /// projection and event subscriptions. Returns `None` when the
    /// plugin doesn't ship the subcommand (silent fall-through to the
    /// legacy shim per SPEC §12) or when its output is unparseable.
    pub fn describe(&self) -> Result<Option<DescribeResponse>> {
        if !self.is_available() {
            return Ok(None);
        }
        let outcome = self.spawn_and_run("describe", &[], &[])?;
        Ok(self.parse_outcome::<DescribeResponse>("describe", outcome))
    }

    /// Native protocol §5.3: plugin proposes its post-event projection
    /// for one task. Returns `None` for any wire failure — the caller
    /// converts that into `AttemptClass::Other`. A successful
    /// `ProposeResponse` may carry either an `ok` or `conflict`
    /// branch; both are handed back unchanged.
    pub fn propose(
        &self,
        event: Event,
        task: &Task,
        ctx_json: Option<&str>,
    ) -> Result<Option<ProposeResponse>> {
        let json = serde_json::to_string(task)?;
        let event_name = event_subcommand_arg(event);
        // SPEC §5.1: only a context-aware plugin gets `--ctx-file`.
        // `_ctx` outlives `spawn_and_run` and RAII-removes the file
        // once the child has exited and read it.
        let ctx = match ctx_json {
            Some(j) => Some(super::ctx::CtxFile::new(j)?),
            None => None,
        };
        let mut flags: Vec<(&str, &str)> = vec![("--event", event_name)];
        if let Some(cf) = &ctx {
            flags.push(("--ctx-file", cf.path_str()));
        }
        let outcome = self.spawn_and_run("propose", &flags, json.as_bytes())?;
        Ok(self.parse_outcome::<ProposeResponse>("propose", outcome))
    }

}

/// Map an `Event` to the lowercase wire name a native plugin
/// receives via `--event`. Stable identifier — same as the serde
/// rename used on the JSON-side `Event` enum.
pub(crate) fn event_subcommand_arg(event: Event) -> &'static str {
    match event {
        Event::Claim => "claim",
        Event::Review => "review",
        Event::Close => "close",
        Event::Update => "update",
        Event::Sync => "sync",
        Event::Create => "create",
        Event::Drop => "drop",
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for p in std::env::split_paths(&paths) {
        let candidate = p.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_subcommand_arg_covers_every_event() {
        // Pin the wire name for every Event variant so a rename
        // changes both this test and the JSON serde rename together.
        assert_eq!(event_subcommand_arg(Event::Claim), "claim");
        assert_eq!(event_subcommand_arg(Event::Review), "review");
        assert_eq!(event_subcommand_arg(Event::Close), "close");
        assert_eq!(event_subcommand_arg(Event::Update), "update");
        assert_eq!(event_subcommand_arg(Event::Sync), "sync");
        assert_eq!(event_subcommand_arg(Event::Create), "create");
        assert_eq!(event_subcommand_arg(Event::Drop), "drop");
    }
}
