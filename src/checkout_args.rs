//! §12/§13 checkout-verb argv parsing — `bl prime` / `bl sync` flags, split
//! from [`crate::checkout`] so the dispatch there stays orchestration (the
//! [`crate::mutate`]/`mutate_args` convention). Both verbs share the §12
//! `--remote`/`--center` precedence: `--remote` always assigns, `--center`
//! fills only an empty slot — the ONE ladder's per-op top tier (bl-c2de).

use std::io;

/// Parsed `bl sync` flags: an optional positional branch, `--as`, and the
/// per-op `--remote`/`--center` override (the §12 ladder's top tier, bl-c2de).
pub(super) struct SyncOpts {
    pub(super) actor: String,
    pub(super) branch: Option<String>,
    pub(super) remote: Option<String>,
}

/// Parsed `bl prime` flags: the resolved actor, the optional store-remote
/// override that becomes the binding's explicit remote (over XDG, §12), the
/// optional `--install CENTER` that triggers config adoption (§13), and
/// `--stealth` — the §12 consent opt-out (sugar for `conf set task-remote
/// none`: the landing sentinel binds every later op, bl-9df0). `install` also seeds
/// the remote when `remote` is unset (the center is where the adopted
/// `tasks_branch` lives), resolved in [`crate::checkout::prime`].
pub(super) struct PrimeOpts {
    pub(super) actor: String,
    pub(super) remote: Option<String>,
    pub(super) install: Option<String>,
    pub(super) stealth: bool,
}

/// Parse `bl prime [--as ID] [--remote URL] [--center URL] [--install CENTER]
/// [--stealth]`. `--remote` and `--center` both name the store remote (the
/// federation framing differs, the effect is one URL); `--remote` wins if both
/// are given, whatever the order (`get_or_insert` lets a later `--center` fill an
/// empty slot but never overwrite a `--remote`, which always assigns).
/// `--install` names the center to adopt config from (§13). `--stealth` opts out
/// of any store remote (§12) and so CONTRADICTS every flag that names one —
/// fail loud, never pick a winner silently. An unknown flag or positional is an
/// error.
pub(super) fn parse_prime(args: &[String], default_actor: &str) -> io::Result<PrimeOpts> {
    let mut o = PrimeOpts { actor: default_actor.to_string(), remote: None, install: None, stealth: false };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--as" => o.actor = value(args, &mut i, "--as")?,
            flag @ ("--remote" | "--center") => apply_remote(&mut o.remote, flag, value(args, &mut i, flag)?),
            "--install" => o.install = Some(value(args, &mut i, "--install")?),
            "--stealth" => o.stealth = true,
            other => return Err(crate::usage(format!("prime: unexpected argument '{other}'"))),
        }
        i += 1;
    }
    if o.stealth && (o.remote.is_some() || o.install.is_some()) {
        return Err(crate::usage(
            "prime: --stealth contradicts --remote/--center/--install — stealth opts out of any store remote",
        ));
    }
    Ok(o)
}

/// Parse `bl sync [BRANCH] [--as ID] [--remote URL] [--center URL]` — the
/// positional is the sync target (§13), the remote flags the shared per-op
/// override (bl-c2de).
pub(super) fn parse_sync(args: &[String], default_actor: &str) -> io::Result<SyncOpts> {
    let mut o = SyncOpts { actor: default_actor.to_string(), branch: None, remote: None };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--as" => o.actor = value(args, &mut i, "--as")?,
            flag @ ("--remote" | "--center") => apply_remote(&mut o.remote, flag, value(args, &mut i, flag)?),
            flag if flag.starts_with('-') => {
                return Err(crate::usage(format!("sync: unexpected flag '{flag}'")));
            }
            _ => {
                if o.branch.replace(args[i].clone()).is_some() {
                    return Err(crate::usage("sync: at most one branch"));
                }
            }
        }
        i += 1;
    }
    Ok(o)
}

/// Apply the §12 `--remote`/`--center` ladder to the store-remote `slot`:
/// `--remote` always assigns, `--center` fills only an empty slot — the ONE
/// precedence rule (bl-c2de) every verb that names a store remote shares
/// (`prime`/`sync` here, the mutating verbs, `import`). `value` is the URL the
/// caller already pulled from argv (cursor styles differ); this owns only the
/// rule, so it cannot drift across the parsers.
pub(crate) fn apply_remote(slot: &mut Option<String>, flag: &str, value: String) {
    if flag == "--center" {
        slot.get_or_insert(value);
    } else {
        *slot = Some(value);
    }
}

/// The value following a `--flag`, advancing the cursor; missing value is an
/// error — the parse step the checkout-lifecycle verbs (and `bl install`,
/// [`crate::install::run`]) share.
pub(crate) fn value(args: &[String], i: &mut usize, flag: &str) -> io::Result<String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| crate::usage(format!("{flag} needs a value")))
}
