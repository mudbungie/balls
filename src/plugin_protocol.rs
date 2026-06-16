//! §6 plugin self-description — `<bin> protocol`. Read at install time to
//! validate a binding (the version handshake), diagnostics otherwise; balls
//! never persists it. Lifted from [`super`] so the dispatch file stays the
//! spawn/wire/recursion machinery.

use std::io;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use super::retry_busy;

/// A plugin's self-description from `<bin> protocol`: the protocol version(s) it
/// speaks and the ops it handles. balls never persists it — it is read at
/// install time to validate a binding, and is diagnostics otherwise (§6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protocol {
    pub protocol: Vec<u32>,
    pub ops: Vec<String>,
}

/// Accept a scalar `protocol: 1` or a list `protocol: [1, 2]` on the wire — both
/// are valid §6 self-descriptions ("version(s)").
#[derive(Deserialize)]
#[serde(untagged)]
enum Versions {
    One(u32),
    Many(Vec<u32>),
}

#[derive(Deserialize)]
struct RawProtocol {
    protocol: Versions,
    ops: Vec<String>,
}

impl Protocol {
    /// Does this plugin speak protocol `version`? The install-time check.
    #[must_use]
    pub fn speaks(&self, version: u32) -> bool {
        self.protocol.contains(&version)
    }
}

/// Run `<bin> protocol` and parse its `{ protocol, ops }` self-description (§6).
/// A spawn failure, a non-zero exit, or unparseable JSON is an [`io::Error`].
pub fn describe(bin: &Path) -> io::Result<Protocol> {
    let out = retry_busy(|| Command::new(bin).arg("protocol").output())?;
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "plugin protocol self-describe exited {}",
            out.status
        )));
    }
    let raw: RawProtocol = serde_json::from_slice(&out.stdout).map_err(io::Error::other)?;
    let protocol = match raw.protocol {
        Versions::One(v) => vec![v],
        Versions::Many(vs) => vs,
    };
    Ok(Protocol { protocol, ops: raw.ops })
}
