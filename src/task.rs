//! §3 task schema — the one node type.
//!
//! There is exactly one node type: **task**. `epic`/`gate`/`bug` are *tags*
//! with zero core behavior; an "epic" is emergent from a task having children.
//! A task is a `tasks/<id>.md` file: a TOML frontmatter block fenced by `+++`,
//! then a free-form markdown body. The id is the filename basename (Model A),
//! so it is NOT a field here — see [`crate::id`].
//!
//! Two things this schema deliberately omits. There is **no `id` field** (the
//! path is the id) and **no `status`/`state` field**: status is a DERIVED view
//! ([`Task::status`]), never stored. Unknown keys round-trip untouched
//! ([`Task::extra`]) — the opt-in seam for a team's own pipeline field.

use serde::{Deserialize, Serialize};

/// A task's frontmatter, plus its markdown body. Mirrors `tasks/<id>.md`; the
/// id lives in the filename, not in here.
///
/// Not `Eq`: `extra` is a [`toml::Table`], whose `Value` can hold a float, so
/// only `PartialEq` is available — fine, we never key a map/set on a `Task`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Task {
    pub title: String,
    /// Unix seconds. Storage/transit is ALWAYS unix-time; only display renders
    /// ISO-8601 (§9). An int needs no date code and sorts numerically.
    pub created: i64,
    pub updated: i64,
    /// Occupancy and its SOLE source of truth: present ⇒ claimed, absent ⇒ not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimant: Option<String>,
    /// Containment pointer only; never read for enforcement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The one ordering input for `bl list`. Lower = higher; absent sorts last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    /// The relational primitive (§10): `{id, on}` edges on the BLOCKED task.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<Blocker>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Unknown frontmatter keys, preserved on writeback. Not part of the
    /// schema core reads — the severable seam for a team's own `state:` field.
    #[serde(flatten)]
    pub extra: toml::Table,
    /// The markdown body after the closing fence. Not frontmatter.
    #[serde(skip)]
    pub body: String,
}

/// A blocker edge: this task can't make op `on` until task `id` resolves (is
/// closed or dropped). `on` is ANY op (§10/§15); `claim` (a dependency) and
/// `close` (a gate) are the two cases with create-time sugar, but the edge gates
/// exactly the op it names — no more, no less.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocker {
    pub id: String,
    pub on: On,
}

/// Which of the blocked task's own ops an edge gates — ANY op (§10/§15). The op
/// an edge names is precisely the op a guard refuses, so `On` IS a [`Verb`]:
/// one type, one source of truth, no claim/close special case. `claim`/`close`
/// are merely the two ops with front-door sugar.
pub type On = crate::verb::Verb;

/// The derived status (§3 ladder) — computed on read, never stored. Exactly
/// three live states; closed/dropped are not states (the file is gone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Claimed,
    Blocked,
    Ready,
}

impl Status {
    /// The three live rungs, ladder order — the §3 single source `word`/`from_word`
    /// both read (the `Verb::ALL` shape, so the token table has one home).
    pub const ALL: [Status; 3] = [Status::Claimed, Status::Blocked, Status::Ready];

    /// The §3 status token — the stable word shared by plain output and `--json`.
    pub fn word(self) -> &'static str {
        match self {
            Status::Claimed => "claimed",
            Status::Blocked => "blocked",
            Status::Ready => "ready",
        }
    }

    /// Parse a status word back to its rung, DERIVED from [`Status::word`] so the
    /// `--status` filter token can never drift from the rendered badge. `None`
    /// for an unknown word (and for `closed` — no live rung, the file is gone).
    pub fn from_word(word: &str) -> Option<Status> {
        Status::ALL.into_iter().find(|s| s.word() == word)
    }
}

/// Why a `tasks/<id>.md` file could not be parsed.
#[derive(Debug)]
pub enum ParseError {
    MissingFrontmatter,
    UnterminatedFrontmatter,
    Toml(toml::de::Error),
    /// A [`SHADOW_KEYS`] member stored as frontmatter — a key whose fact lives
    /// OUTSIDE the frontmatter, so a stored line is a shadow the bedrock
    /// projection silently drops (a lossy hole in the §9 round-trip). Refused
    /// at parse, the one gate every write funnels through (the §8.3 seal
    /// re-parse, `--edit`'s buffer validation, every read).
    ReservedKey(&'static str),
}

/// The frontmatter keys no `tasks/<id>.md` may carry: each names a fact whose
/// one authoritative home is elsewhere — `id` is the filename (§3 Model A) and
/// `body` is the markdown after the closing fence. These are the only canonical
/// names serde's flatten could capture as extras (every other §3 field is a
/// struct field, so TOML's duplicate-key rule already excludes a twin).
const SHADOW_KEYS: [&str; 2] = ["id", "body"];

impl Task {
    /// Parse a `tasks/<id>.md` file: a `+++`-fenced TOML frontmatter block, then
    /// the markdown body. The id is NOT read here — it is the filename.
    pub fn parse(content: &str) -> Result<Task, ParseError> {
        let rest = content
            .strip_prefix("+++\n")
            .ok_or(ParseError::MissingFrontmatter)?;
        let (frontmatter, body) = match rest.split_once("\n+++\n") {
            Some((frontmatter, body)) => (frontmatter, body),
            None => (
                rest.strip_suffix("\n+++")
                    .ok_or(ParseError::UnterminatedFrontmatter)?,
                "",
            ),
        };
        let mut task: Task = toml::from_str(frontmatter).map_err(ParseError::Toml)?;
        if let Some(key) = SHADOW_KEYS.into_iter().find(|k| task.extra.contains_key(*k)) {
            return Err(ParseError::ReservedKey(key));
        }
        task.body = body.to_string();
        Ok(task)
    }

    /// Render back to `tasks/<id>.md` form. Round-trips with [`Task::parse`],
    /// unknown keys included.
    ///
    /// # Panics
    /// Only if the frontmatter fails to serialize to TOML, which a well-formed
    /// in-memory `Task` never does.
    pub fn to_markdown(&self) -> String {
        let frontmatter = toml::to_string(self).expect("task frontmatter serializes");
        format!("+++\n{frontmatter}+++\n{}", self.body)
    }

    /// The §3 status ladder, computed on read. `is_resolved(id)` answers
    /// whether a blocker's task has resolved (closed or dropped — file gone);
    /// the store supplies it. The ladder short-circuits: a claimant wins
    /// outright (claim-blockers are then moot), else any unresolved
    /// CLAIM-blocker means blocked, else ready. Close-blockers never show.
    pub fn status(&self, is_resolved: &dyn Fn(&str) -> bool) -> Status {
        if self.claimant.is_some() {
            Status::Claimed
        } else if self
            .blockers
            .iter()
            .any(|b| b.on == On::Claim && !is_resolved(&b.id))
        {
            Status::Blocked
        } else {
            Status::Ready
        }
    }

    /// §10 `ready()`: claimable now — unclaimed with every CLAIM-blocker
    /// resolved. Exactly the [`Status::Ready`] rung of the ladder, named for the
    /// `bl list --status ready` question it answers. Enforcement is CORE (§10):
    /// the `claim` op
    /// rejects a non-ready ball ([`crate::change::Occupancy`]).
    pub fn ready(&self, is_resolved: &dyn Fn(&str) -> bool) -> bool {
        matches!(self.status(is_resolved), Status::Ready)
    }

    /// §10 `closeable()`: every CLOSE-blocker (gate) resolved — enforced by core
    /// at the `close` op ([`crate::change::Retire`]), before any `close.pre`
    /// plugin runs. Independent of the claim ladder: a close-blocker never shows
    /// as a status, it only gates the finish.
    pub fn closeable(&self, is_resolved: &dyn Fn(&str) -> bool) -> bool {
        !self
            .blockers
            .iter()
            .any(|b| b.on == On::Close && !is_resolved(&b.id))
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::MissingFrontmatter => f.write_str("missing opening `+++` frontmatter fence"),
            ParseError::UnterminatedFrontmatter => {
                f.write_str("unterminated frontmatter: no closing `+++` fence")
            }
            ParseError::Toml(e) => write!(f, "invalid frontmatter: {e}"),
            ParseError::ReservedKey(k) => write!(
                f,
                "reserved frontmatter key '{k}' — the id is the filename and the body follows the fence; neither is stored frontmatter"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
