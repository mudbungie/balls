//! §6 plugin wiring — the `config/plugins.toml` `[hooks]` schedule.
//!
//! The hook list is config: `config/plugins.toml`'s `[hooks]` table on the
//! landing (§2) is the SINGLE source of truth for which plugin runs in which
//! op-phase. A `<op>.<phase>` key maps to an ORDERED LIST of plugin names —
//! listed = run, list position = run order (the last name runs last). An absent
//! key or empty list = run nothing (the general path with no entries, §4). This
//! retires the filesystem `<op>/<phase>/NN-<name>` symlink registry: ordering is
//! a list property, not an `NN-` filename convention faking one.
//!
//! Names are committed text — portable verbatim, valid in stealth and federation
//! regardless of where the checkout sits. The LOCAL `config/plugins/bin/<name>`
//! symlink ([`crate::registry`]) resolves each name to this machine's binary;
//! [`Hooks::resolve`] stitches the two halves into the [`PluginRef`] sets the §8
//! engine runs, an absent `bin/<name>` surfacing as a dangling ref (a clean
//! "referenced but not installed here" at dispatch, never a silent skip).

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;

use crate::registry::{PluginRef, Registry};

/// The parsed `[hooks]` table: `"<op>.<phase>"` → its ordered plugin-name list.
/// A [`BTreeMap`] so the schedule (and its [`Hooks::referenced`] projection) has
/// a deterministic order — the seed re-serializes it after pruning (§12).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Hooks {
    table: BTreeMap<String, Vec<String>>,
}

impl Hooks {
    /// Parse a `plugins.toml` body's `[hooks]` table. A missing `[hooks]` table,
    /// or a value that is not a string array, contributes no entries; a malformed
    /// TOML document is an error. balls reads only `[hooks]`; any other table a
    /// team adds round-trips untouched on `install` (a file copy) but is ignored
    /// here.
    pub fn parse(body: &str) -> io::Result<Hooks> {
        let root: toml::Table = toml::from_str(body).map_err(io::Error::other)?;
        Ok(match root.get("hooks") {
            Some(toml::Value::Table(hooks)) => Hooks::from_hooks_table(hooks),
            _ => Hooks::default(),
        })
    }

    /// Build the schedule from an already-extracted `[hooks]` sub-table — the
    /// shared tail of [`Hooks::parse`] and the layered [`Hooks::effective`]. A
    /// value that is not a string array contributes no names.
    fn from_hooks_table(hooks: &toml::Table) -> Hooks {
        let mut table = BTreeMap::new();
        for (key, value) in hooks {
            let names = value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect();
            table.insert(key.clone(), names);
        }
        Hooks { table }
    }

    /// The EFFECTIVE dispatch schedule (§4/§6, bl-8540): the landing's `[hooks]`
    /// overlaid by the per-machine XDG `plugins.toml`'s `[hooks]`, merged like
    /// every other config list — a bare `<op>.<phase>` REPLACES, and
    /// `_prepend`/`_append`/`_ban` COMPOSE ([`crate::config::layer_over`]), XDG
    /// (innermost) winning. So a box composes a center's committed schedule with
    /// its own machine-local one, the §4 ONE-layering-mechanism rather than a
    /// second registry. An absent layer contributes nothing. This is the DISPATCH
    /// read; the seed and `install` read the committed landing schedule alone via
    /// [`Hooks::load`]/[`Hooks::load_from`] (the XDG overlay is dispatch-only — it
    /// must not redirect what the seed prunes or `install` binds).
    pub fn effective(landing: &Path, user_config: &Path) -> io::Result<Hooks> {
        let mut merged = toml::value::Table::new();
        if let Some(hooks) = hooks_layer(&landing.join("config").join("plugins.toml"))? {
            crate::config::layer_over(&mut merged, hooks);
        }
        if let Some(hooks) = hooks_layer(&user_config.with_file_name("plugins.toml"))? {
            crate::config::layer_over(&mut merged, hooks);
        }
        Ok(Hooks::from_hooks_table(&merged))
    }

    /// Load the `[hooks]` schedule from `plugins.toml` at `path`. An absent file
    /// is the un-wired case — an empty schedule (run nothing), not an error.
    pub fn load_from(path: &Path) -> io::Result<Hooks> {
        match std::fs::read_to_string(path) {
            Ok(body) => Hooks::parse(&body),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Hooks::default()),
            Err(e) => Err(e),
        }
    }

    /// Load the schedule from a landing's `config/plugins.toml` (§2). The
    /// dispatch + rebind entry point ([`crate::mutate`]/[`crate::checkout`]).
    pub fn load(landing: &Path) -> io::Result<Hooks> {
        Hooks::load_from(&landing.join("config").join("plugins.toml"))
    }

    /// The ordered plugin names under one schedule `key` (empty when un-wired).
    fn key_names(&self, key: &str) -> &[String] {
        self.table.get(key).map_or(&[], Vec::as_slice)
    }

    /// The ordered plugin names wired for `<op>.<phase>` (empty when un-wired).
    #[must_use]
    pub fn names(&self, op: &str, phase: &str) -> &[String] {
        self.key_names(&format!("{op}.{phase}"))
    }

    /// Resolve `<op>.<phase>` into the engine's [`PluginRef`] set, in list order,
    /// each name stitched to its local `bin/<name>` via `registry` (`None` when
    /// not installed here — a dangling ref the dispatch rejects, §6).
    #[must_use]
    pub fn resolve(&self, registry: &Registry, op: &str, phase: &str) -> Vec<PluginRef> {
        Self::refs(registry, self.names(op, phase))
    }

    /// Resolve a READ op's plugin set (§6 read dispatch): a read carries no seal
    /// and no `pre`/`post` split, so its hook key is the BARE `<op>` token — one
    /// key for the one phase it dispatches.
    #[must_use]
    pub fn resolve_read(&self, registry: &Registry, op: &str) -> Vec<PluginRef> {
        Self::refs(registry, self.key_names(op))
    }

    /// Stitch `names` to their local `bin/<name>` bindings, in list order.
    fn refs(registry: &Registry, names: &[String]) -> Vec<PluginRef> {
        names
            .iter()
            .map(|name| PluginRef { name: name.clone(), bin: registry.resolve_bin(name) })
            .collect()
    }

    /// Every plugin the schedule names, mapped to the op tokens it is wired into
    /// — the `<op>` half of each `<op>.<phase>` key. The seed binds each of these
    /// to its sibling binary (§12); `bl install` validates each against the local
    /// binary's self-description (§6).
    #[must_use]
    pub fn referenced(&self) -> BTreeMap<String, BTreeSet<String>> {
        let mut refs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (key, names) in &self.table {
            let op = key.split('.').next().unwrap_or(key);
            for name in names {
                refs.entry(name.clone()).or_default().insert(op.to_string());
            }
        }
        refs
    }

    /// Drop every name failing `keep` from every list — the seed's prune of
    /// entries whose binary is absent here, so a box missing a default plugin
    /// never aborts (§12).
    pub fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        for names in self.table.values_mut() {
            names.retain(|name| keep(name));
        }
    }

    /// Serialize back to a `plugins.toml` body — a single `[hooks]` table with
    /// the surviving entries (an emptied list is dropped: empty = run nothing).
    /// The seed writes this after [`Hooks::retain`] prunes the absent binaries.
    ///
    /// # Panics
    /// Only if the `[hooks]` table fails to serialize to TOML, which a table of
    /// string arrays never does.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut hooks = toml::value::Table::new();
        for (key, names) in &self.table {
            if !names.is_empty() {
                let array = names.iter().cloned().map(toml::Value::String).collect();
                hooks.insert(key.clone(), toml::Value::Array(array));
            }
        }
        let mut root = toml::value::Table::new();
        root.insert("hooks".to_string(), toml::Value::Table(hooks));
        toml::to_string(&toml::Value::Table(root)).expect("a hooks table always serializes")
    }
}

/// Read one `plugins.toml` layer's `[hooks]` sub-table for [`Hooks::effective`].
/// An absent file ⇒ `None` (the layer contributes nothing); a present file with a
/// missing or non-table `[hooks]` ⇒ an empty table (also nothing, but distinct
/// from absent); a malformed document ⇒ an error naming the file.
fn hooks_layer(path: &Path) -> io::Result<Option<toml::Table>> {
    match std::fs::read_to_string(path) {
        Ok(body) => {
            let root: toml::Table =
                toml::from_str(&body).map_err(|e| io::Error::other(format!("{}: {e}", path.display())))?;
            Ok(Some(match root.get("hooks") {
                Some(toml::Value::Table(hooks)) => hooks.clone(),
                _ => toml::Table::new(),
            }))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
#[path = "hooks_tests.rs"]
mod tests;
