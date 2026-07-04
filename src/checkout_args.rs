//! §12/§13 checkout-verb argv parsing — `bl prime` / `bl sync` flags, split
//! from [`crate::checkout`] so the dispatch there stays orchestration (the
//! [`crate::mutate`]/`mutate_args` convention). `--remote` is the per-op store-
//! remote override every store-touching verb shares (the ONE §12 ladder's top
//! tier, bl-c2de); `--center` is PRIME-ONLY and means enrollment, not an alias
//! (bl-35e5) — the rule is `--remote` shapes one op, `--center` enrolls a checkout.

use std::io;

/// Parsed `bl sync` flags: an optional positional branch, `--as`, and the
/// per-op `--remote` override (the §12 ladder's top tier, bl-c2de).
pub(super) struct SyncOpts {
    pub(super) actor: String,
    pub(super) branch: Option<String>,
    pub(super) remote: Option<String>,
}

/// Parsed `bl prime` flags: the resolved actor, the per-op `--remote` store-
/// remote override (over XDG for this op, §12), the `--install CENTER` that
/// triggers config adoption (§13), the `--center URL` that ENROLLS this checkout
/// (bl-35e5 — the durable binding write + config adoption in one, prime-only),
/// and `--stealth` — the §12 consent opt-out (sugar for `conf set task-remote
/// none`: the landing sentinel binds every later op, bl-9df0). `install` also
/// seeds the remote when `remote` is unset (the center is where the adopted
/// `tasks_branch` lives), resolved in [`crate::checkout::prime`].
pub(super) struct PrimeOpts {
    pub(super) actor: String,
    pub(super) remote: Option<String>,
    pub(super) install: Option<String>,
    pub(super) center: Option<String>,
    pub(super) stealth: bool,
}

/// Parse `bl prime [--as ID] [--remote URL] [--center URL] [--install CENTER]
/// [--stealth]`. `--remote URL` is the per-op store-remote override, unchanged.
/// `--center URL` ENROLLS this checkout (bl-35e5): the durable per-clone binding
/// write PLUS config adoption from the center — so it SUBSUMES `--install`, and
/// the two are mutually exclusive (both name the center whose config we adopt;
/// one flag enrolls, so passing both is a redundant/contradictory pair — fail
/// loud, don't guess which URL wins). `--install` names the center to adopt config
/// from WITHOUT the durable binding (§13). `--stealth` opts out of any store
/// remote (§12) and so CONTRADICTS every flag that names one — fail loud, never
/// pick a winner silently. An unknown flag or positional is an error.
pub(super) fn parse_prime(args: &[String], default_actor: &str) -> io::Result<PrimeOpts> {
    let mut o = PrimeOpts { actor: default_actor.to_string(), remote: None, install: None, center: None, stealth: false };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--as" => o.actor = value(args, &mut i, "--as")?,
            "--remote" => o.remote = Some(value(args, &mut i, "--remote")?),
            "--center" => o.center = Some(value(args, &mut i, "--center")?),
            "--install" => o.install = Some(value(args, &mut i, "--install")?),
            "--stealth" => o.stealth = true,
            other => return Err(crate::usage(format!("prime: unexpected argument '{other}'"))),
        }
        i += 1;
    }
    if o.center.is_some() && o.install.is_some() {
        return Err(crate::usage(
            "prime: --center already adopts CENTER's config — pass --center (enroll) or --install (adopt only), not both",
        ));
    }
    if o.stealth && (o.remote.is_some() || o.center.is_some() || o.install.is_some()) {
        return Err(crate::usage(
            "prime: --stealth contradicts --remote/--center/--install — stealth opts out of any store remote",
        ));
    }
    Ok(o)
}

/// Parse `bl sync [BRANCH] [--as ID] [--remote URL]` — the positional is the sync
/// target (§13), `--remote` the per-op store-remote override (bl-c2de). `--center`
/// is NOT accepted here: it enrolls a checkout (prime-only, bl-35e5), so on `sync`
/// it falls through to the unexpected-flag error like any other unknown flag.
pub(super) fn parse_sync(args: &[String], default_actor: &str) -> io::Result<SyncOpts> {
    let mut o = SyncOpts { actor: default_actor.to_string(), branch: None, remote: None };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--as" => o.actor = value(args, &mut i, "--as")?,
            "--remote" => o.remote = Some(value(args, &mut i, "--remote")?),
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

/// The value following a `--flag`, advancing the cursor; missing value is an
/// error — the parse step the checkout-lifecycle verbs (and `bl install`,
/// [`crate::install::run`]) share.
pub(crate) fn value(args: &[String], i: &mut usize, flag: &str) -> io::Result<String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| crate::usage(format!("{flag} needs a value")))
}
