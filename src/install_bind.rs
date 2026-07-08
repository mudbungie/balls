//! §6 install's LOCAL-BINDING half — resolve every name the landed schedule
//! references (and the configured `clock_provider`) to THIS machine's binary and
//! link it under `config/plugins/bin/<name>`.
//!
//! Lifted out of the run-wiring ([`super`], the §9 decomposition convention):
//! that sibling owns the git-sealing copy, this one owns the git-free local
//! resolution the copy hands off to. Two idioms live here, deliberately
//! separate: the protocol-validated PLUGIN loop ([`bind_referenced`] →
//! [`crate::install::resolve_and_bind`], refusing an op/protocol the binary does
//! not declare) and the `clock_provider` ([`bind_clock`]), a bindable name that
//! speaks NO plugin protocol because it resolves an INPUT (the op clock, §8) not
//! an effect — validated as a clock ([`crate::clock::probe`]) instead. A name
//! resolvable to no binary stays dangling and is REPORTED ([`report_dangling`]),
//! never bound silently (§6, bl-5b09).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::EffectiveConfig;
use crate::edge::Edge;
use crate::hooks::Hooks;
use crate::install::{referenced, resolve_and_bind, InstallError, Summary};
use crate::log::{Level, Log};
use crate::message::PROTOCOL;
use crate::registry::Registry;

use std::io;

/// The §6 BIND-ONLY fallback taken when no copy source resolved (no `--from`, an
/// empty `FETCH_HEAD`): a stealth/hub box with binding work to do — an explicit
/// `--bin` or a configured `clock_provider` — skips the config copy (there is
/// nothing to adopt) and reports an empty [`Summary`] so the caller falls
/// straight through to [`bind_referenced`]. With NO binding work it is the
/// original refusal, naming the remedy. (Reaching here means `--from` was
/// absent, which the parser only permits landing-targeted, so the target is
/// invariantly the landing — no `--to` re-check, no dead branch.)
pub(super) fn bind_only(landing: &Path, edge: &Edge, bins: &BTreeMap<String, PathBuf>) -> io::Result<Summary> {
    let configured = EffectiveConfig::resolve(landing, &edge.xdg.user_config())?.clock_provider.is_some();
    if !bins.is_empty() || configured {
        return Ok(Summary::default());
    }
    Err(io::Error::other(
        "install: no --from given and no configured upstream offers a balls/config to adopt — pass --from <ref>",
    ))
}

/// Bind every plugin the just-landed `config/plugins.toml` references to this
/// machine's binary, validating each against its live `<bin> protocol`
/// self-description before linking (§6 [`resolve_and_bind`] — refuses an op
/// or protocol version the binary does not declare, the refusal carrying the
/// name's `[source]` hint when one exists: it doubles as the stale-binary
/// upgrade pointer, bl-5b09). The candidate is the explicit `--bin
/// <name>=<path>` entry when given (a `bins` name neither the schedule nor the
/// `clock_provider` names is refused — never silently dropped), else
/// [`locate`]'s machine lookup. A referenced name with no candidate anywhere
/// stays dangling — the clean "referenced but not installed" dispatch error
/// (§6), never bound silently — and is REPORTED ([`report_dangling`]); a re-run
/// converges on the no-op seal and just binds (§14). The configured
/// `clock_provider` binds LAST, in its own [`bind_clock`] idiom.
pub(crate) fn bind_referenced(landing: &Path, edge: &Edge, bins: &BTreeMap<String, PathBuf>, log: &Log) -> io::Result<()> {
    let worklist = referenced(landing)?;
    let clock = EffectiveConfig::resolve(landing, &edge.xdg.user_config())?.clock_provider;
    // A `--bin` name must be a name the landing RESOLVES: a scheduled plugin, or
    // the configured `clock_provider` (a bindable name that speaks no protocol).
    if let Some(name) = bins.keys().find(|n| !worklist.contains_key(*n) && Some(n.as_str()) != clock.as_deref()) {
        return Err(io::Error::other(format!(
            "install: --bin {name}: the landed schedule does not reference that plugin"
        )));
    }
    let hints = Hooks::effective(landing, &edge.xdg.user_config())?;
    let registry = Registry::at(landing);
    for (name, ops) in worklist {
        let source = hints.source(&name).map(|h| format!(" — source: {h}")).unwrap_or_default();
        let Some(bin) = bins.get(&name).cloned().or_else(|| locate(&name, edge)) else {
            report_dangling(log, &name, &source);
            continue;
        };
        resolve_and_bind(&registry, &name, &bin, &ops, PROTOCOL).map_err(|e| match e {
            InstallError::Unsupported { .. } => io::Error::other(format!("{e}{source}")),
            InstallError::Io(_) => io::Error::other(e.to_string()),
        })?;
    }
    if let Some(name) = clock {
        bind_clock(&registry, &name, bins, edge, &hints, log)?;
    }
    Ok(())
}

/// The §6 "referenced but not bound" dangling report — one `info` line naming
/// the missing binary and the re-run remedy, the name's `[source]` hint appended
/// when authored (bl-5b09). Shared by the protocol-validated plugin loop and the
/// [`bind_clock`] provider bind so both dangle byte-identically.
fn report_dangling(log: &Log, name: &str, source: &str) {
    log.record(
        Level::Info,
        "core",
        None,
        &format!("install: {name} referenced but not bound (no binary beside bl or on PATH){source} — re-run bl install after acquiring"),
    );
}

/// Bind the configured `clock_provider` (§4/§8) in its OWN idiom, SEPARATE from
/// the protocol-validated plugin loop: it resolves an INPUT (the op clock), not
/// an effect, so it speaks no plugin protocol. Resolve its binary (explicit
/// `--bin` else the machine [`locate`]); absent ⇒ the shared [`report_dangling`];
/// present ⇒ VALIDATE it as a clock — [`crate::clock::probe`] runs it and
/// requires exactly one parseable unix-seconds line, exit 0 — then link it. A
/// provider that errors or prints a non-integer is REFUSED (never bound); the
/// bl-8b98 fail-open ladder then degrades that op to the system clock.
fn bind_clock(registry: &Registry, name: &str, bins: &BTreeMap<String, PathBuf>, edge: &Edge, hints: &Hooks, log: &Log) -> io::Result<()> {
    let source = hints.source(name).map(|h| format!(" — source: {h}")).unwrap_or_default();
    let Some(bin) = bins.get(name).cloned().or_else(|| locate(name, edge)) else {
        report_dangling(log, name, &source);
        return Ok(());
    };
    crate::clock::probe(&bin).map_err(|e| io::Error::other(format!("install: refusing to bind clock_provider {name}: {e}")))?;
    registry.bind(name, &bin)
}

/// §6 "this machine" resolution for a referenced name's binary: the shipped
/// sibling beside `bl` first (the seed's own rule, [`crate::seed`] — a
/// freshly built `bl` finds its co-built plugins even off PATH), then a PATH
/// lookup (`edge.path_dirs`). No hit ⇒ `None` — the caller leaves the name
/// dangling.
pub(crate) fn locate(name: &str, edge: &Edge) -> Option<PathBuf> {
    let dirs = edge.exe_dir.iter().chain(edge.path_dirs.iter());
    dirs.map(|d| d.join(name)).find(|p| p.is_file())
}
