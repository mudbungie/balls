//! §6 `bl install` argv — the parse layer, a sibling of the run-wiring (the
//! [`crate::checkout_args`] convention; bl-4c45).
//!
//! `bl install [<path>] [--from <ref>] [--to <ref>] [--bin <name>=<path>]…
//! [--as ID]`. The §6 defaults: `<path>` is the recommended bundle
//! ([`DEFAULT_PATH`] — all of `config/`, never the store) and must stay inside
//! the checkout (relative, no `..`); `--to` defaults to the landing; `--from`
//! defaults to the CONFIGURED UPSTREAM — parse leaves it `None` and the run
//! wiring resolves it through the `install.pre` fetch chain (core itself still
//! reaches no remote, §0). Two guards are parse-time because both legs belong
//! to the LANDING-TARGETED direction (§6): a store-targeted install names its
//! `--from` explicitly (the upstream default adopts config INTO the landing),
//! and `--bin` feeds the landing-side binary resolution — an explicit flag
//! silently dropped would be the bl-cf93 sin, so each is refused, not ignored.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::checkout;
use crate::install::DEFAULT_PATH;
use crate::LANDING_BRANCH;

/// Parsed `bl install` argv. `from: None` = the §6 configured-upstream
/// default, resolved by the run wiring; `bins` maps a plugin name to the
/// explicit `--bin <name>=<path>` candidate that outranks the machine lookup.
#[derive(Debug)]
pub(crate) struct Opts {
    pub(crate) path: String,
    pub(crate) from: Option<String>,
    pub(crate) to: String,
    pub(crate) actor: String,
    pub(crate) bins: BTreeMap<String, PathBuf>,
}

/// Parse install's argv (see the module doc for the §6 defaults and guards).
pub(crate) fn parse(args: &[String], default_actor: &str) -> io::Result<Opts> {
    let (mut path, mut from, mut to) = (None, None, None);
    let mut bins = BTreeMap::new();
    let mut actor = default_actor.to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--from" => from = Some(checkout::value(args, &mut i, "--from")?),
            "--to" => to = Some(checkout::value(args, &mut i, "--to")?),
            "--as" => actor = checkout::value(args, &mut i, "--as")?,
            "--bin" => {
                let v = checkout::value(args, &mut i, "--bin")?;
                let (name, bin) = v
                    .split_once('=')
                    .ok_or_else(|| io::Error::other(format!("install: --bin wants <name>=<path>, got '{v}'")))?;
                bins.insert(name.to_string(), PathBuf::from(bin));
            }
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
    let to = to.unwrap_or_else(|| LANDING_BRANCH.to_string());
    if to != LANDING_BRANCH {
        if from.is_none() {
            return Err(io::Error::other(
                "install: --from is required when --to is not the landing (the configured-upstream default adopts into the landing, §6)",
            ));
        }
        if !bins.is_empty() {
            return Err(io::Error::other(
                "install: --bin applies only to a landing-targeted install (§6 local binary resolution)",
            ));
        }
    }
    Ok(Opts { path, from, to, actor, bins })
}

#[cfg(test)]
#[path = "install_args_tests.rs"]
mod tests;
