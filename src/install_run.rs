//! §6/§8 `bl install` run-wiring — parse, materialize the two roots, SEAL the
//! path-copy through the engine, bind, report.
//!
//! [`crate::install`] is the pure path-copy; this sibling gives it the §8
//! sealing shape. A throwaway detached worktree materializes `--from` (any
//! local ref — `<ref>` is a *synced* repo/branch, §6; the fetch that syncs a
//! remote one is the tracker's `install.pre`); [`Copier`] stages the copy into
//! the change worktree the engine opens on `--to`'s CURRENT tip; the seal
//! commits + ff-integrates it, swapping only `<path>` — never a whole-tree
//! replace, never a ref reset (§6 "siblings are never touched") — with §14
//! rollback on any abort. Identical bytes stage nothing and seal to the
//! existing tip (the no-op seal, §13 idempotence). `--to` resolves to one of
//! the two LOCAL checkouts (the landing or the store, §2); sealing to a remote
//! center is the open bl-66e7 question, so any other ref is refused, not
//! guessed. A landing-targeted install then resolves + binds every plugin the
//! landed schedule references ([`bind_referenced`]) and prints the [`Summary`]
//! — the §6 blast radius, on stdout.
//!
//! `prime --install` (§13, [`crate::adopt`]) converges on the same spine: its
//! tracker fetch runs FIRST (the engine stages before `pre`, so a fetch the
//! staging depends on cannot ride the engine's own chain — the one M3 leg
//! still outside the engine trace), then [`seal_copy`] + [`bind_referenced`]
//! do the rest. One copy path, one seal.

use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};

use crate::checkout;
use crate::edge::Edge;
use crate::git::{self, Git};
use crate::hooks::Hooks;
use crate::install::{install, referenced, resolve_and_bind, Summary, DEFAULT_PATH};
use crate::layout::CloneDir;
use crate::lifecycle::{BaseChange, Engine};
use crate::log::{self, Log};
use crate::message::PROTOCOL;
use crate::plugin::Subprocess;
use crate::registry::{PluginRef, Registry};
use crate::safegit::reject_option_like;
use crate::verb::Verb;
use crate::wire::OpContext;
use crate::LANDING_BRANCH;

/// `bl install [<path>] --from <ref> [--to <ref>] [--as ID]` (§6): seal a copy
/// of `<path>` from `--from` onto `--to`'s current tip through the §8 engine +
/// the `install` plugin chain, then bind the referenced plugins when the
/// landing was the target, and print the change [`Summary`].
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
    let plugins = Subprocess::new(OpContext::diffless(opts.actor, binding), &log, edge.depth);
    let chain = Chain {
        plugins: &plugins,
        log: &log,
        pre: hooks.resolve(&reg, Verb::Install.token(), "pre"),
        post: hooks.resolve(&reg, Verb::Install.token(), "post"),
    };
    let to = if to_landing { &landing } else { &store };
    let summary = seal_copy(&clone, &opts.path, &opts.from, to, &chain)?;
    if to_landing {
        bind_referenced(&landing, edge.exe_dir.as_deref())?;
    }
    println!("install: {summary}");
    Ok(())
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
pub(crate) fn seal_copy(clone: &CloneDir, path: &str, from: &str, to: &Path, chain: &Chain) -> io::Result<Summary> {
    reject_option_like(from)?;
    let src = source_root(to, clone, from)?;
    let base = Copier {
        path,
        src: &src,
        message: format!("balls: install {path} --from {from}"),
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
/// `finalize` is the install commit message — no §5 trailers, install carries
/// none of the task-shaped fields (§8). `copied` carries the [`Summary`] back
/// across the engine's `&dyn` seam for the caller to print.
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
/// machine's sibling binary beside `bl` (`exe_dir`), validating each against
/// its live `<bin> protocol` self-description before linking (§6
/// [`resolve_and_bind`] — refuses an op or protocol version the binary does
/// not declare). A referenced name with no sibling here stays dangling — the
/// clean "referenced but not installed" dispatch error (§6), never bound
/// silently. `exe_dir == None` ⇒ a plugin-free box (nothing to bind).
pub(crate) fn bind_referenced(landing: &Path, exe_dir: Option<&Path>) -> io::Result<()> {
    let registry = Registry::at(landing);
    for (name, ops) in referenced(landing)? {
        if let Some(bin) = exe_dir.map(|d| d.join(&name)).filter(|p| p.exists()) {
            resolve_and_bind(&registry, &name, &bin, &ops, PROTOCOL)
                .map_err(|e| io::Error::other(e.to_string()))?;
        }
    }
    Ok(())
}

/// Parsed `bl install [<path>] --from <ref> [--to <ref>] [--as ID]`.
#[derive(Debug)]
struct Opts {
    path: String,
    from: String,
    to: String,
    actor: String,
}

/// Parse install's argv. `path` defaults to the recommended bundle
/// ([`DEFAULT_PATH`] — all of `config/`, never the store, §6) and must stay
/// inside the checkout (relative, no `..`); `--from` is required — core
/// resolves no implicit upstream (§0); `--to` defaults to the landing (§6).
fn parse(args: &[String], default_actor: &str) -> io::Result<Opts> {
    let (mut path, mut from, mut to) = (None, None, None);
    let mut actor = default_actor.to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--from" => from = Some(checkout::value(args, &mut i, "--from")?),
            "--to" => to = Some(checkout::value(args, &mut i, "--to")?),
            "--as" => actor = checkout::value(args, &mut i, "--as")?,
            flag if flag.starts_with('-') => {
                return Err(io::Error::other(format!("install: unexpected flag '{flag}'")));
            }
            p => {
                if path.replace(p.to_string()).is_some() {
                    return Err(io::Error::other("install: at most one path"));
                }
            }
        }
        i += 1;
    }
    let path = path.unwrap_or_else(|| DEFAULT_PATH.to_string());
    if Path::new(&path).is_absolute() || path.split('/').any(|c| c == "..") {
        return Err(io::Error::other(format!("install: path must be checkout-relative: '{path}'")));
    }
    let from = from.ok_or_else(|| io::Error::other("install: --from <ref> is required"))?;
    let opts = Opts { path, from, to: to.unwrap_or_else(|| LANDING_BRANCH.to_string()), actor };
    Ok(opts)
}

#[cfg(test)]
#[path = "install_run_tests.rs"]
mod tests;
