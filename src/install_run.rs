//! §6/§8 `bl install` run-wiring — parse, materialize the two roots, SEAL the
//! path-copy through the engine, bind, report.
//!
//! [`crate::install`] is the pure path-copy; this sibling gives it the §8
//! sealing shape. A throwaway detached worktree materializes `--from` (any
//! local ref — `<ref>` is a *synced* repo/branch, §6); [`Copier`] stages the
//! copy into the change worktree the engine opens on `--to`'s CURRENT tip; the
//! seal commits + ff-integrates it, swapping only `<path>` — never a
//! whole-tree replace, never a ref reset (§6 "siblings are never touched") —
//! with §14 rollback on any abort. Identical bytes stage nothing and seal to
//! the existing tip (the no-op seal, §13 idempotence). `--to` resolves to one
//! of the two LOCAL checkouts (the landing or the store, §2); sealing to a
//! remote center is the open bl-66e7 question, so any other ref is refused,
//! not guessed.
//!
//! An omitted `--from` is the §6 default — the CONFIGURED UPSTREAM, resolved
//! by [`upstream`]: the `install.pre` chain (the tracker, the only
//! remote-talker, §0) fetches the upstream's `balls/config` to the landing's
//! `FETCH_HEAD` and the copy stages from that git-standard ref. The chain must
//! run BEFORE the engine stages (staging reads the ref the fetch leaves, so
//! the fetch cannot ride the engine's own `pre` — the [`crate::adopt`]
//! precedent, the one leg outside the §14 trace), so the engine's pre chain
//! then runs EMPTY. `prime --install` converges on the same spine: its tracker
//! fetch runs first, then [`seal_copy`] + [`bind_referenced`] do the rest.
//!
//! A landing-targeted install then resolves + binds every plugin the landed
//! schedule references ([`bind_referenced`]: explicit `--bin <name>=<path>`,
//! else the `bl` sibling, else PATH — §6 local binary resolution) and prints
//! the [`Summary`] — the §6 blast radius, on stdout. Binding runs AFTER the
//! seal, deliberately (bl-4c45): the copy is committed TEXT (the §6
//! recommendation — adopting it is the consent step), binding is this box's
//! LOCAL resolution of it, so a validation refusal exits non-zero with the
//! schedule landed and `bin/<name>` dangling — exactly §6's clean "referenced
//! but not installed" state. The commit is the undo; a retry with a fixed
//! binary converges on the no-op seal and just binds (§14).

use std::cell::Cell;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::checkout;
use crate::edge::Edge;
use crate::git::{self, Git};
use crate::hooks::Hooks;
use crate::install::{install, referenced, resolve_and_bind, Summary};
use crate::layout::CloneDir;
use crate::lifecycle::{BaseChange, Engine, Plugins};
use crate::log::{self, Log};
use crate::message::{Message, PROTOCOL};
use crate::op::Phase;
use crate::plugin::Subprocess;
use crate::registry::{PluginRef, Registry};
use crate::safegit::reject_option_like;
use crate::verb::Verb;
use crate::wire::OpContext;
use crate::LANDING_BRANCH;

use args::parse;

/// `bl install [<path>] [--from <ref>] [--to <ref>] [--bin <name>=<path>]…
/// [--as ID]` (§6): seal a copy of `<path>` from `--from` (default: the
/// configured upstream) onto `--to`'s current tip through the §8 engine + the
/// `install` plugin chain, then bind the referenced plugins when the landing
/// was the target, and print the change [`Summary`].
pub fn run(edge: &Edge, args: &[String]) -> io::Result<()> {
    let opts = parse(args, &edge.default_actor)?;
    let clone = edge.xdg.clone_dir(&edge.invocation_path);
    let (landing, store) = (clone.landing(), clone.store());
    if !landing.join("config").is_dir() {
        return Err(io::Error::other("no balls checkout here — run `bl prime` first"));
    }
    let (binding, level) = checkout::bind(edge, &landing, &store, None, None, false)?;
    let to_landing = opts.to == LANDING_BRANCH;
    if !to_landing && opts.to != binding.tasks_branch {
        return Err(io::Error::other(format!(
            "install: --to must name the landing ({LANDING_BRANCH}) or the configured store branch ({}) — any other seal target is not wired (bl-66e7)",
            binding.tasks_branch
        )));
    }
    let hooks = Hooks::effective(&landing, &edge.xdg.user_config())?;
    let reg = Registry::at(&landing);
    let log = Log::new(clone.op_log(), level, Verb::Install, log::wall);
    let plugins = Subprocess::new(OpContext::diffless(opts.actor.clone(), binding), &log, edge.depth);
    let pre = hooks.resolve(&reg, Verb::Install.token(), "pre");
    let (pre, from) = match opts.from {
        Some(ref f) => (pre, f.clone()),
        None => (Vec::new(), upstream(&plugins, &pre, &landing)?),
    };
    let chain = Chain {
        plugins: &plugins,
        log: &log,
        pre,
        post: hooks.resolve(&reg, Verb::Install.token(), "post"),
    };
    let to = if to_landing { &landing } else { &store };
    let summary = seal_copy(&clone, &opts.path, &from, to, &chain, &opts.actor)?;
    if to_landing {
        bind_referenced(&landing, edge, &opts.bins)?;
    }
    println!("install: {summary}");
    Ok(())
}

/// Resolve the §6 `--from` default — the CONFIGURED UPSTREAM. Core reaches no
/// remote itself (§0): the `install.pre` chain (the tracker, §13 — remote =
/// the one §12 ladder) fetches the upstream's `balls/config` into the
/// landing's `FETCH_HEAD`, and the copy stages from that git-standard ref —
/// the same fetch-then-local-copy split as `prime --install`
/// ([`crate::adopt`]). The chain runs HERE, before the engine stages (see the
/// module doc); the caller then runs the engine's pre chain EMPTY. A
/// `FETCH_HEAD` that still does not resolve (a stealth box, a hub carrying no
/// `balls/config`, no fetch plugin installed) is refused naming the remedy —
/// never a raw git fatal at materialize.
fn upstream(plugins: &Subprocess, pre: &[PluginRef], landing: &Path) -> io::Result<String> {
    for plugin in pre {
        plugins.run(plugin, Verb::Install, Phase::Pre, landing, None)?;
    }
    if git::run(landing, &["rev-parse", "--verify", "--quiet", "FETCH_HEAD"], None).is_err() {
        return Err(io::Error::other(
            "install: no --from given and no configured upstream offers a balls/config to adopt — pass --from <ref>",
        ));
    }
    Ok("FETCH_HEAD".to_string())
}

/// The resolved §8 pieces a sealing install runs with: the subprocess chain,
/// the op log, and the `install.pre`/`install.post` plugin sets.
pub(crate) struct Chain<'a> {
    pub(crate) plugins: &'a Subprocess<'a>,
    pub(crate) log: &'a Log,
    pub(crate) pre: Vec<PluginRef>,
    pub(crate) post: Vec<PluginRef>,
}

/// SEAL `<path>` from the local ref `from` onto the `to` checkout's CURRENT
/// tip through the §8 engine — the ONE sealing path `bl install` and
/// `prime --install` share. Materializes `from` as a throwaway detached
/// worktree (the copy's source root), drives [`Engine::seal`] with [`Copier`]
/// as the base change, tears the source down whatever the outcome, and
/// returns the change [`Summary`].
pub(crate) fn seal_copy(clone: &CloneDir, path: &str, from: &str, to: &Path, chain: &Chain, actor: &str) -> io::Result<Summary> {
    reject_option_like(from)?;
    let src = source_root(to, clone, from)?;
    let base = Copier {
        path,
        src: &src,
        message: Message::checkout(Verb::Install, actor, format!("balls: install {path} --from {from}")).render()?,
        copied: Cell::new(Summary::default()),
    };
    let change = clone.change("install");
    // Pre-clean a crashed run's leftover change worktree so the op RESUMES.
    let _ = git::run(to, &["worktree", "remove", "--force", &change.to_string_lossy()], None);
    let sealed = Engine::new(&Git::at(to), chain.plugins, chain.log)
        .seal(&base, Verb::Install, &change, &chain.pre, &chain.post);
    let _ = git::run(to, &["worktree", "remove", "--force", &src.to_string_lossy()], None);
    sealed.map_err(|e| io::Error::other(e.to_string()))?;
    Ok(base.copied.get())
}

/// Materialize `from` as a throwaway detached worktree — the copy's committed
/// source root. Force-recreated so a crashed install never trips on a stale
/// one; `repo` is any checkout of the one substrate repo (§2).
fn source_root(repo: &Path, clone: &CloneDir, from: &str) -> io::Result<PathBuf> {
    let src = clone.change("install-src");
    let s = src.to_string_lossy().into_owned();
    let _ = git::run(repo, &["worktree", "remove", "--force", &s], None);
    git::run(repo, &["worktree", "add", "--detach", &s, from], None)?;
    Ok(src)
}

/// install's [`BaseChange`]: `stage` is the §6 path-copy into the change
/// worktree (opened on `--to`'s tip, so the seal swaps only `<path>`);
/// `finalize` is the pre-rendered §5 message — checkout-scoped, so it carries
/// the protocol/op/actor trailers but no `bl-id` (bl-1d9b; the task-shaped
/// WIRE fields stay absent, §8). `copied` carries the [`Summary`] back across
/// the engine's `&dyn` seam for the caller to print.
struct Copier<'a> {
    path: &'a str,
    src: &'a Path,
    message: String,
    copied: Cell<Summary>,
}

impl BaseChange for Copier<'_> {
    fn stage(&self, dir: &Path) -> io::Result<()> {
        self.copied.set(install(self.path, self.src, dir)?);
        Ok(())
    }

    fn finalize(&self, _dir: &Path) -> io::Result<String> {
        Ok(self.message.clone())
    }
}

/// Bind every plugin the just-landed `config/plugins.toml` references to this
/// machine's binary, validating each against its live `<bin> protocol`
/// self-description before linking (§6 [`resolve_and_bind`] — refuses an op
/// or protocol version the binary does not declare). The candidate is the
/// explicit `--bin <name>=<path>` entry when given (a `bins` name the schedule
/// does not reference is refused — never silently dropped), else [`locate`]'s
/// machine lookup. A referenced name with no candidate anywhere stays dangling
/// — the clean "referenced but not installed" dispatch error (§6), never
/// bound silently.
pub(crate) fn bind_referenced(landing: &Path, edge: &Edge, bins: &BTreeMap<String, PathBuf>) -> io::Result<()> {
    let worklist = referenced(landing)?;
    if let Some(name) = bins.keys().find(|n| !worklist.contains_key(*n)) {
        return Err(io::Error::other(format!(
            "install: --bin {name}: the landed schedule does not reference that plugin"
        )));
    }
    let registry = Registry::at(landing);
    for (name, ops) in worklist {
        if let Some(bin) = bins.get(&name).cloned().or_else(|| locate(&name, edge)) {
            resolve_and_bind(&registry, &name, &bin, &ops, PROTOCOL)
                .map_err(|e| io::Error::other(e.to_string()))?;
        }
    }
    Ok(())
}

/// §6 "this machine" resolution for a referenced plugin's binary: the shipped
/// sibling beside `bl` first (the seed's own rule, [`crate::seed`] — a
/// freshly built `bl` finds its co-built plugins even off PATH), then a PATH
/// lookup (`edge.path_dirs`). No hit ⇒ `None` — the caller leaves the name
/// dangling.
fn locate(name: &str, edge: &Edge) -> Option<PathBuf> {
    let dirs = edge.exe_dir.iter().chain(edge.path_dirs.iter());
    dirs.map(|d| d.join(name)).find(|p| p.is_file())
}

// The argv parser lives in a sibling module (the §9 checkout_args convention).
#[path = "install_args.rs"]
mod args;

#[cfg(test)]
#[path = "install_run_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "install_surface_tests.rs"]
mod surface_tests;
