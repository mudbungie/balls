//! `bl-delivery` — the §11 delivery / worktree plugin binary (direct variant).
//!
//! A thin process edge over [`balls::delivery`]: it answers `protocol` for the
//! §6 self-describe, otherwise gathers the §7 wire (stdin), the §6 env
//! (`BALLS_PLUGIN_NAME` + XDG), and argv (`<op> <phase>`), resolves the derived
//! worktree, and runs the hook. All policy lives in the library (the
//! [`balls::delivery::dispatch`] matrix + the [`balls::delivery_repo::Project`]
//! git seam); `main` only adapts the boundary, the way `bl` does.

use std::env;
use std::io::{self, Read};
use std::path::Path;
use std::process::exit;

use balls::delivery::{self, Repo, Spec, Wire};
use balls::delivery_precondition::{precondition_unmet, require_repo};
use balls::delivery_repo::{changed_task_paths, claimed_ids, Project};
use balls::layout::Xdg;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("protocol") {
        println!("{}", delivery::PROTOCOL_JSON);
        return;
    }
    if let Err(e) = run(&args) {
        eprintln!("bl-delivery: {e}");
        exit(1);
    }
}

/// Gather the boundary inputs and run the hook. The op/phase are argv; the wire
/// is stdin; the plugin name + XDG bases are env (resolved here, never in the
/// library — the layout layer takes them as arguments).
fn run(args: &[String]) -> io::Result<()> {
    let op = args.first().ok_or_else(|| io::Error::other("usage: bl-delivery <op> <phase>"))?;
    let phase = args.get(1).ok_or_else(|| io::Error::other("usage: bl-delivery <op> <phase>"))?;

    let mut stdin = String::new();
    io::stdin().read_to_string(&mut stdin)?;
    let wire: Wire = serde_json::from_str(&stdin).map_err(io::Error::other)?;
    delivery::ensure_safe_invocation_path(&wire.binding.invocation_path)?;

    let plugin = var("BALLS_PLUGIN_NAME")?;
    let home = var("HOME")?;
    let xdg = Xdg::with(Path::new(&home), env::var("XDG_CONFIG_HOME").ok().as_deref(), env::var("XDG_STATE_HOME").ok().as_deref());

    let invocation = &wire.binding.invocation_path;
    let repo = Project::at(Path::new(invocation));

    // `prime` carries no single ball (§13 diffless) — it re-materializes one
    // worktree per still-claimed ball, so it takes its own path here.
    if op == "prime" {
        return prime(phase, &wire, &xdg, &plugin, &repo);
    }

    let cwd = env::current_dir()?;
    let id = delivery::resolve_id(wire.metadata.as_ref(), || changed_task_paths(&cwd))?;

    let worktree = delivery::worktree_path(&xdg, &plugin, invocation, &id);
    let branch = delivery::work_branch(&id);
    let rolling_back = wire.rolling_back.is_some();

    let title = wire.current_state.as_ref().map_or("", |s| s.title.as_str());
    let subject = delivery::subject(title, &id);
    let marker = delivery::marker(&id);
    let spec = Spec {
        worktree: &worktree,
        branch: &branch,
        subject: &subject,
        marker: &marker,
    };
    // bl-4a88: the delivery precondition gate — claim.post / close.pre abort
    // cleanly here when `root` is not a git repo, in balls' voice, rather than
    // git's raw `fatal: not a git repository` from the first worktree act.
    require_repo(op, phase, rolling_back, &repo, invocation)?;
    delivery::dispatch(op, phase, rolling_back, &repo, &spec)?;
    // §11 surfacing on stdout, forwarded/folded by balls (§6): `claim.post`
    // prints the just-materialized path (the verb's one product); the `show`
    // read-op prints the worktree field line for the named ball iff the worktree
    // exists on this machine. Nothing is stored — the path is recomputed here.
    if let Some(line) = delivery::surfaced(op, phase, rolling_back, &worktree, worktree.is_dir()) {
        println!("{line}");
    }
    Ok(())
}

/// `prime.post` re-materialization (§11/§12): for every ball in the store
/// checkout still claimed by the actor, run the same `materialize` act a
/// `claim.post` would, behind the dispatch matrix — then PRINT each worktree's
/// path (§11: prime is the resume moment, so it re-surfaces what claim printed).
/// The claimed set replaces the single derived id; each worktree is recomputed
/// from `(invocation, id)`, so a re-prime whose worktrees already exist is a
/// no-op (create-if-absent). The store is the diffless cwd balls invokes us in
/// (§13), not a wire field. Once the claimed set is re-materialized (their
/// branches now checked out, so the prune cannot touch them) prime PRUNES the
/// settled `work/<id>` branches close/unclaim teardown left behind — the §11
/// deferred, non-transactional branch cleanup ([`Project::prune`]).
fn prime(phase: &str, wire: &Wire, xdg: &Xdg, plugin: &str, repo: &Project) -> io::Result<()> {
    // §14: prime is an idempotent refresher — a re-materialized worktree is
    // exactly the state a re-prime converges to, so its rollback DECLINES
    // before touching anything (bl-62eb). Declining first also matters
    // mechanically: the unwind invokes rollbacks with cwd = the LANDING, which
    // has no tasks/, so the claimed-set scan below would die with ENOENT.
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
    let store = env::current_dir()?;
    for id in claimed_ids(&store, &wire.actor)? {
        let worktree = delivery::worktree_path(xdg, plugin, &wire.binding.invocation_path, &id);
        let branch = delivery::work_branch(&id);
        let spec = Spec { worktree: &worktree, branch: &branch, subject: "", marker: "" };
        delivery::dispatch("prime", phase, false, repo, &spec)?;
        if let Some(line) = delivery::surfaced("prime", phase, false, &worktree, worktree.is_dir()) {
            println!("{line}");
        }
    }
    if phase == "post" {
        repo.prune()?;
    }
    Ok(())
}

/// Read a required env var, mapping absence to a clear protocol error.
fn var(key: &str) -> io::Result<String> {
    env::var(key).map_err(|_| io::Error::other(format!("{key} is unset (set by balls per §6)")))
}
