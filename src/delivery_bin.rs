//! The delivery plugin's process boundary as a LIBRARY entrypoint.
//!
//! [`crate::delivery`] is the pure hook→act policy; `src/bin/bl-delivery.rs` is
//! its shipped process edge. This module is the SAME boundary adaptation —
//! argv (`protocol`, or `<op> <phase>`), the §7 wire off `input`, the §11
//! surfacing onto `out`, errors in the plugin's own voice — promoted `pub` so a
//! LINKING host can answer the `bl-delivery` argv/wire contract from the crate
//! (the yog U-balls-3 ask: a host that self-multiplexes `bl` ships no sibling
//! binaries beside its exe, so it must be able to *be* the sibling). The
//! shipped binary and the linking host both call [`run`]; neither carries a
//! second copy of the boundary, which is the point.
//!
//! Env reads stay OUT of this module (the bl-bfa8 rule): the caller resolves
//! `$BALLS_PLUGIN_NAME`, the XDG bases, and the working directory once at its
//! own process edge and hands them in as [`Env`] — the same shape
//! [`crate::tracker::Env`] gives the tracker plugin.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::delivery::{self, Repo as _, Spec};
use crate::delivery_path;
use crate::delivery_precondition::{precondition_unmet, require_repo};
use crate::delivery_repo::Project;
use crate::delivery_wire::Wire;
use crate::layout::Xdg;

/// The host-resolved inputs one `bl-delivery` invocation needs, read once at
/// the caller's process boundary (bl-bfa8: no env reads in the lib).
pub struct Env {
    /// `$BALLS_PLUGIN_NAME` as read (`None` ⇒ unset). Required for every hook
    /// op — it names the plugin's §1 state territory — but not for `protocol`,
    /// which balls invokes bare at install time.
    pub plugin: Option<String>,
    /// The XDG bases behind the plugin's worktree territory (§1/§11).
    pub xdg: Xdg,
    /// The process working directory — the STORE checkout core runs
    /// `prime.post` in (§13 diffless), the only cwd any hook still reads now
    /// that identity rides the wire (bl-a5f3).
    pub cwd: PathBuf,
}

/// The delivery-plugin entrypoint: answer `protocol` on `out`, else run the
/// `<op> <phase>` hook with the §7 wire read from `input`, returning the
/// process exit code. A hook error is printed to stderr in the plugin's voice
/// (`bl-delivery: …`) and becomes exit `1` — the §6 "non-zero aborts the op".
pub fn run(args: &[String], input: &mut impl Read, out: &mut impl Write, env: &Env) -> i32 {
    if args.first().map(String::as_str) == Some("protocol") {
        let _ = writeln!(out, "{}", delivery::PROTOCOL_JSON);
        return 0;
    }
    match hook(args, input, out, env) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("bl-delivery: {e}");
            1
        }
    }
}

/// Gather the boundary inputs and run the hook. The op/phase are argv; the wire
/// is `input`; the plugin name, XDG bases and cwd arrive resolved in [`Env`].
fn hook(args: &[String], input: &mut impl Read, out: &mut impl Write, env: &Env) -> io::Result<()> {
    let op = args.first().ok_or_else(|| io::Error::other("usage: bl-delivery <op> <phase>"))?;
    let phase = args.get(1).ok_or_else(|| io::Error::other("usage: bl-delivery <op> <phase>"))?;

    let mut stdin = String::new();
    input.read_to_string(&mut stdin)?;
    let wire: Wire = serde_json::from_str(&stdin).map_err(io::Error::other)?;
    delivery_path::ensure_safe_invocation_path(&wire.binding.invocation_path)?;

    let plugin = env
        .plugin
        .as_deref()
        .ok_or_else(|| io::Error::other("BALLS_PLUGIN_NAME is unset (set by balls per §6)"))?;

    let invocation = &wire.binding.invocation_path;
    let repo = Project::at(Path::new(invocation));

    // `prime` carries no single ball (§13 diffless) — it derives no worktree
    // (worktrees materialize at CLAIM only, bl-c2bf), only prunes settled
    // `work/<id>` branches (+ reports debris on the unsettled ones, bl-c117),
    // so it takes its own path here. The STORE checkout it runs in is its cwd —
    // the one act that still reads one.
    if op == "prime" {
        return prime(phase, &wire, &repo, &env.xdg, plugin, &env.cwd);
    }

    // §0 obligation 4: the ball is an op INPUT off the wire, never re-derived
    // from the change worktree's staged diff (bl-a5f3).
    let id = delivery::resolve_id(wire.metadata.as_ref(), wire.command.as_ref().and_then(|c| c.id.as_deref()))?;

    let worktree = delivery_path::worktree_path(&env.xdg, plugin, invocation, &id);
    let branch = delivery_path::work_branch(&id);
    let rolling_back = wire.rolling_back.is_some();

    let title = wire.current_state.as_ref().map_or("", |s| s.title.as_str());
    let subject = delivery_path::subject(title, &id);
    let marker = delivery_path::marker(&id);
    // bl-9961: a close's `-m` note is free BODY narration under the tagged
    // subject (never a subject override, §5).
    let override_msg = wire.command.as_ref().and_then(|c| c.message.as_deref());
    // bl-7b71: the delivery target core derived from the graph — an id, which
    // only here becomes the `work/<id>` ref. Absent ⇒ the integration branch.
    let target = wire.command.as_ref().and_then(|c| c.target.as_deref());
    let spec = Spec {
        worktree: &worktree,
        branch: &branch,
        subject: &subject,
        override_msg,
        marker: &marker,
        target,
    };
    // bl-4a88: the delivery precondition gate — claim.post / close.pre abort
    // cleanly here when `root` is not a git repo, in balls' voice, rather than
    // git's raw `fatal: not a git repository` from the first worktree act.
    require_repo(op, phase, rolling_back, &repo, invocation)?;
    delivery::dispatch(op, phase, rolling_back, &repo, &spec)?;
    // §11 surfacing on `out`, forwarded/folded by balls (§6): `claim.post`
    // prints the just-materialized path (the verb's one product); the `show`
    // read-op prints the worktree field line for the named ball iff the worktree
    // exists on this machine. Nothing is stored — the path is recomputed here.
    if let Some(line) = delivery::surfaced(op, phase, rolling_back, &worktree, worktree.is_dir()) {
        writeln!(out, "{line}")?;
    }
    Ok(())
}

/// `prime.post` housekeeping (§11/§12): worktrees materialize at CLAIM and
/// nowhere else (bl-c2bf — re-priming a lost worktree is `unclaim` + `claim`),
/// so prime derives no worktree at all. It PRUNES the settled `work/<id>`
/// branches close/unclaim teardown left behind — the §11 deferred,
/// non-transactional branch cleanup ([`Project::prune`]) — and REPORTS (never
/// prunes) the unsettled ones whose worktree directory is gone (bl-c117: piece
/// 3 of docs/design/bl-18bf-prime-convergence.md). `xdg`/`plugin` are the same
/// binding inputs `claim` resolves its own worktree path from; `cwd` is the
/// STORE checkout core runs `prime.post` in (§13 diffless), which is how the
/// report tells an open ball's debris from a closed one's (bl-baa0) — and the
/// only cwd any hook still reads, now that identity rides the wire (bl-a5f3).
fn prime(phase: &str, wire: &Wire, repo: &Project, xdg: &Xdg, plugin: &str, cwd: &Path) -> io::Result<()> {
    // §14: prime is an idempotent refresher — its prune is exactly the state a
    // re-prime converges to, so its rollback DECLINES before touching anything
    // (bl-62eb).
    if wire.rolling_back.is_some() {
        return Ok(());
    }
    // bl-4a88: a non-repo invocation path makes delivery unusable. WARN once, at
    // founding (before any task is filed) — and no-op, do NOT abort prime (the
    // house style: prime warns, never refuses). The per-ball gate
    // ([`require_repo`]) aborts later if you claim/close anyway.
    if !repo.is_git_repo()? {
        eprintln!("bl-delivery: {}", precondition_unmet(&wire.binding.invocation_path));
        return Ok(());
    }
    if phase == "post" {
        for line in repo.prune(xdg, plugin, cwd)? {
            eprintln!("bl-delivery: {line}");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "delivery_bin_tests.rs"]
mod tests;
