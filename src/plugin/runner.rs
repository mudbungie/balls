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
        let executable = plugin_executable(name);
        // `config_file` is clone-root-relative (SPEC §7, bl-1d81).
        // The conventional `.balls/plugins/` subtree is a symlink into
        // the state checkout, so the join lands the path on the
        // tracker-tracked file regardless of whether the clone
        // bind-mounts the state checkout or holds its own copy.
        let config_path = store.plugin_config_root().join(&entry.config_file);
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
        // A clap-built legacy plugin rejects `describe` with
        // `error: unrecognized subcommand 'describe'` and exits non-zero.
        // The fall-through is intentional per SPEC §12, so don't let
        // parse_outcome leak a misleading "describe failed" warning.
        if !outcome.status.success()
            && String::from_utf8_lossy(&outcome.stderr).contains("unrecognized subcommand")
        {
            return Ok(None);
        }
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
        let mut flags: Vec<(&str, &str)> = vec![("--event", &event_name)];
        if let Some(cf) = &ctx {
            flags.push(("--ctx-file", cf.path_str()));
        }
        let outcome = self.spawn_and_run("propose", &flags, json.as_bytes())?;
        Ok(self.parse_outcome::<ProposeResponse>("propose", outcome))
    }

}

/// The lowercase wire name a native plugin receives via `--event`.
/// Derived straight from `Event`'s `#[serde(rename_all = "lowercase")]`
/// so the CLI flag and the JSON-side encoding share one source of
/// truth and a variant rename cannot drift the two apart.
pub(crate) fn event_subcommand_arg(event: Event) -> String {
    let json = serde_json::to_value(event).expect("Event always serializes");
    let name = json.as_str().expect("Event serializes to a JSON string");
    name.to_string()
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

/// The executable bl spawns for plugin `name`: `balls-plugin-{name}`,
/// resolved against `PATH`. In test builds an active
/// [`test_seam::ExecutableOverride`] substitutes a path of the test's
/// choosing, so "plugin unavailable" tests can fail resolution without
/// the process-global `env::remove_var("PATH")` that raced concurrent
/// subprocess-spawning tests (bl-ad4b, same class as bl-bfa8).
fn plugin_executable(name: &str) -> String {
    #[cfg(test)]
    if let Some(exe) = test_seam::executable_override() {
        return exe;
    }
    format!("balls-plugin-{name}")
}

/// Test-only seam for `Plugin::resolve`: lets a test redirect the
/// plugin executable to a path it controls instead of stripping
/// `PATH`, which is process-global and races any concurrent test that
/// shells out.
#[cfg(test)]
pub(crate) mod test_seam {
    use std::cell::RefCell;
    use std::path::Path;

    thread_local! {
        /// Active override for the current test's thread. Thread-local,
        /// so a test setting it stays invisible to tests on other
        /// harness threads — the isolation `env::set_var` cannot give.
        static OVERRIDE: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    /// The executable an active [`ExecutableOverride`] imposes, if any.
    pub(super) fn executable_override() -> Option<String> {
        OVERRIDE.with(|o| o.borrow().clone())
    }

    /// RAII guard that points `Plugin::resolve` at an absolute,
    /// never-created executable under `dir` (pass a tempdir the test
    /// owns). `which()` and spawn then both fail deterministically with
    /// `PATH` left untouched. The override clears on drop.
    #[must_use]
    pub(crate) struct ExecutableOverride;

    impl ExecutableOverride {
        pub(crate) fn unresolvable(dir: impl AsRef<Path>) -> Self {
            let exe = dir.as_ref().join("balls-plugin-absent");
            OVERRIDE.with(|o| {
                *o.borrow_mut() = Some(exe.to_string_lossy().into_owned());
            });
            ExecutableOverride
        }
    }

    impl Drop for ExecutableOverride {
        fn drop(&mut self) {
            OVERRIDE.with(|o| *o.borrow_mut() = None);
        }
    }
}
