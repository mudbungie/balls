//! §6 machine-layer dispatch resolution (bl-053a) — the PATH fallback for
//! names the XDG `plugins.toml` contributed.
//!
//! Locality of the naming layer decides the resolution surface. A name in the
//! LANDING's committed schedule travels with the branch and `bl install`, so it
//! resolves only through the landing's explicit `config/plugins/bin/<name>`
//! binding — an adopted config must never silently run a same-named `$PATH`
//! binary. A name the MACHINE's own XDG `plugins.toml` contributed never
//! travels, exactly like `clock_provider` (bl-cfe3), so it gets the same
//! "this machine" lookup the clock gets: beside `bl` first (the seed sibling
//! rule), then `$PATH` — no `bin/<name>` symlink to hand-bind per landing,
//! because the layer that named it IS this box's own trust statement.
//!
//! Without this, a machine-globally wired plugin aborted every op in every
//! landing (and every future prime) until each was bound with `bl install
//! --bin` — which made the XDG layer unusable for its one job.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// The names an XDG `[hooks]` table CONTRIBUTES: every string in every list
/// value, except those under a `_ban` directive — a ban removes a name from
/// the schedule, so it grants nothing. Non-array values and non-string entries
/// contribute nothing, mirroring [`super::Hooks::parse`]'s tolerance.
pub(super) fn contributed(hooks: &toml::value::Table) -> BTreeSet<String> {
    hooks
        .iter()
        .filter(|(key, _)| !key.ends_with("_ban"))
        .flat_map(|(_, value)| value.as_array().into_iter().flatten())
        .filter_map(|entry| entry.as_str().map(str::to_string))
        .collect()
}

/// The machine fallback for [`super::Hooks::bound`]: `name` must be a machine
/// (XDG-layer) contribution, and then the first `dirs` entry holding it as a
/// file wins — `dirs` is beside-`bl` first, then `$PATH`, as the edge resolved
/// them. A landing-only name, or a name no dir holds, is `None` (the unbound
/// refusal stands).
pub(super) fn fallback(names: &BTreeSet<String>, dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    if !names.contains(name) {
        return None;
    }
    dirs.iter().map(|d| d.join(name)).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use crate::hooks::Hooks;
    use crate::registry::Registry;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    const LANDING: &str = "[hooks]\n\"close.post\" = [\"tracker\"]\n";

    /// Lay a landing schedule, an XDG `plugins.toml`, and a machine lookup dir
    /// holding `bins` as plain files; return what `bound` needs.
    fn machine(xdg: &str, bins: &[&str]) -> (TempDir, Hooks) {
        let tmp = TempDir::new().unwrap();
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        fs::write(config.join("plugins.toml"), LANDING).unwrap();
        let xdg_dir = tmp.path().join("xdg");
        fs::create_dir_all(&xdg_dir).unwrap();
        fs::write(xdg_dir.join("plugins.toml"), xdg).unwrap();
        let dir = tmp.path().join("machine-bin");
        fs::create_dir_all(&dir).unwrap();
        for bin in bins {
            fs::write(dir.join(bin), "#!/bin/sh\n").unwrap();
        }
        let hooks = Hooks::effective(tmp.path(), &xdg_dir.join("config.toml"), &[dir]).unwrap();
        (tmp, hooks)
    }

    #[test]
    fn xdg_contributed_name_falls_back_to_the_machine_dirs() {
        let (tmp, hooks) = machine("[hooks]\n\"close.post_prepend\" = [\"gate\"]\n", &["gate"]);
        let bin = hooks.bound(&Registry::at(tmp.path()), "gate").expect("machine fallback resolves");
        assert!(bin.ends_with("machine-bin/gate"));
        assert_eq!(hooks.names("close", "post"), ["gate", "tracker"]);
    }

    #[test]
    fn landing_name_never_reaches_the_machine_dirs() {
        // `tracker` sits in the machine dir, but only the LANDING names it — the
        // traveling schedule must not silently run a same-named PATH binary.
        let (tmp, hooks) = machine("[hooks]\n\"close.pre\" = [\"lint\"]\n", &["tracker", "lint"]);
        assert_eq!(hooks.bound(&Registry::at(tmp.path()), "tracker"), None);
        assert!(hooks.bound(&Registry::at(tmp.path()), "lint").is_some());
    }

    #[test]
    fn a_ban_directive_contributes_no_machine_name() {
        let (tmp, hooks) = machine("[hooks]\n\"close.post_ban\" = [\"tracker\"]\n", &["tracker"]);
        assert_eq!(hooks.bound(&Registry::at(tmp.path()), "tracker"), None, "a ban grants nothing");
        assert_eq!(hooks.names("close", "post"), [] as [&str; 0]);
    }

    #[test]
    fn the_registry_binding_outranks_the_machine_lookup() {
        let (tmp, hooks) = machine("[hooks]\n\"close.post_prepend\" = [\"gate\"]\n", &["gate"]);
        let real = tmp.path().join("bound-gate");
        fs::write(&real, "#!/bin/sh\n").unwrap();
        let registry = Registry::at(tmp.path());
        registry.bind("gate", &real).unwrap();
        assert_eq!(hooks.bound(&registry, "gate"), Some(real.canonicalize().unwrap()));
    }

    #[test]
    fn a_name_no_dir_holds_stays_unbound() {
        let (tmp, hooks) = machine("[hooks]\n\"close.post_prepend\" = [\"gate\"]\n", &[]);
        assert_eq!(hooks.bound(&Registry::at(tmp.path()), "gate"), None);
    }

    #[test]
    fn a_parse_read_carries_no_machine_trust() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("machine-bin");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("gate"), "").unwrap();
        let hooks = Hooks::parse("[hooks]\n\"close.post\" = [\"gate\"]\n").unwrap();
        assert_eq!(hooks.bound(&Registry::at(tmp.path()), "gate"), None);
    }

    #[test]
    fn machine_dirs_probe_in_order() {
        let tmp = TempDir::new().unwrap();
        let (first, second) = (tmp.path().join("first"), tmp.path().join("second"));
        for d in [&first, &second] {
            fs::create_dir_all(d).unwrap();
            fs::write(d.join("gate"), "").unwrap();
        }
        let config = tmp.path().join("config");
        fs::create_dir_all(&config).unwrap();
        let xdg_dir = tmp.path().join("xdg");
        fs::create_dir_all(&xdg_dir).unwrap();
        fs::write(xdg_dir.join("plugins.toml"), "[hooks]\n\"close.post\" = [\"gate\"]\n").unwrap();
        let dirs: Vec<PathBuf> = vec![first.clone(), second];
        let hooks = Hooks::effective(tmp.path(), &xdg_dir.join("config.toml"), &dirs).unwrap();
        assert_eq!(hooks.bound(&Registry::at(tmp.path()), "gate"), Some(first.join("gate")));
    }
}
