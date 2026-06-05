//! §3 task schema — the one node type.
//!
//! There is exactly one node type: **task**. `epic`/`gate`/`bug` are *tags*
//! with zero core behavior; an "epic" is emergent from a task having children.
//! A task is a `tasks/<id>.md` file: a YAML frontmatter block fenced by `---`,
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
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Task {
    pub title: String,
    /// RFC3339 UTC. Kept as text — it round-trips verbatim and sorts lexically.
    pub created: String,
    pub updated: String,
    /// Occupancy and its SOLE source of truth: present ⇒ claimed, absent ⇒ not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claimant: Option<String>,
    /// Containment pointer only; never read for enforcement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The one ordering input for `bl ready`. Lower = higher; absent sorts last.
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
    pub extra: serde_yaml_ng::Mapping,
    /// The markdown body after the closing fence. Not frontmatter.
    #[serde(skip)]
    pub body: String,
}

/// A blocker edge: this task can't make transition `on` until task `id`
/// resolves (is closed or dropped). The two sugar-supported uses are a
/// dependency (`on: claim`) and a gate (`on: close`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocker {
    pub id: String,
    pub on: On,
}

/// Which of the blocked task's own transitions an edge gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum On {
    Claim,
    Close,
}

/// The derived status (§3 ladder) — computed on read, never stored. Exactly
/// three live states; closed/dropped are not states (the file is gone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Claimed,
    Blocked,
    Ready,
}

/// Why a `tasks/<id>.md` file could not be parsed.
#[derive(Debug)]
pub enum ParseError {
    MissingFrontmatter,
    UnterminatedFrontmatter,
    Yaml(serde_yaml_ng::Error),
}

impl Task {
    /// Parse a `tasks/<id>.md` file: a `---`-fenced YAML frontmatter block, then
    /// the markdown body. The id is NOT read here — it is the filename.
    pub fn parse(content: &str) -> Result<Task, ParseError> {
        let rest = content
            .strip_prefix("---\n")
            .ok_or(ParseError::MissingFrontmatter)?;
        let (frontmatter, body) = match rest.split_once("\n---\n") {
            Some((frontmatter, body)) => (frontmatter, body),
            None => (
                rest.strip_suffix("\n---")
                    .ok_or(ParseError::UnterminatedFrontmatter)?,
                "",
            ),
        };
        let mut task: Task = serde_yaml_ng::from_str(frontmatter).map_err(ParseError::Yaml)?;
        task.body = body.to_string();
        Ok(task)
    }

    /// Render back to `tasks/<id>.md` form. Round-trips with [`Task::parse`],
    /// unknown keys included.
    ///
    /// # Panics
    /// Only if the frontmatter fails to serialize to YAML, which a well-formed
    /// in-memory `Task` never does.
    pub fn to_markdown(&self) -> String {
        let frontmatter = serde_yaml_ng::to_string(self).expect("task frontmatter serializes");
        format!("---\n{frontmatter}---\n{}", self.body)
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
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::MissingFrontmatter => f.write_str("missing opening `---` frontmatter fence"),
            ParseError::UnterminatedFrontmatter => {
                f.write_str("unterminated frontmatter: no closing `---` fence")
            }
            ParseError::Yaml(e) => write!(f, "invalid frontmatter: {e}"),
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = concat!(
        "---\n",
        "title: Refactor the foo system\n",
        "created: 2026-05-27T14:32:00Z\n",
        "updated: 2026-05-28T09:15:00Z\n",
        "claimant: orionriver@gmail.com\n",
        "parent: bl-1000\n",
        "priority: 2\n",
        "blockers:\n",
        "- id: bl-1100\n",
        "  on: claim\n",
        "- id: bl-1200\n",
        "  on: close\n",
        "tags:\n",
        "- refactor\n",
        "- infra\n",
        "---\n",
        "Free-form markdown body.\n",
    );

    fn unresolved(_: &str) -> bool {
        false
    }

    #[test]
    fn parses_every_field_and_the_body() {
        let task = Task::parse(FULL).unwrap();
        assert_eq!(task.title, "Refactor the foo system");
        assert_eq!(task.created, "2026-05-27T14:32:00Z");
        assert_eq!(task.claimant.as_deref(), Some("orionriver@gmail.com"));
        assert_eq!(task.parent.as_deref(), Some("bl-1000"));
        assert_eq!(task.priority, Some(2));
        assert_eq!(
            task.blockers,
            vec![
                Blocker { id: "bl-1100".into(), on: On::Claim },
                Blocker { id: "bl-1200".into(), on: On::Close },
            ]
        );
        assert_eq!(task.tags, ["refactor", "infra"]);
        assert_eq!(task.body, "Free-form markdown body.\n");
    }

    #[test]
    fn full_task_round_trips_byte_for_byte() {
        assert_eq!(Task::parse(FULL).unwrap().to_markdown(), FULL);
    }

    #[test]
    fn a_minimal_task_omits_optionals_and_has_no_body() {
        let src = "---\ntitle: t\ncreated: c\nupdated: u\n---\n";
        let task = Task::parse(src).unwrap();
        assert_eq!(task.claimant, None);
        assert!(task.blockers.is_empty());
        assert_eq!(task.body, "");
        assert_eq!(task.to_markdown(), src);
    }

    #[test]
    fn unknown_keys_survive_a_round_trip() {
        let src = "---\ntitle: t\ncreated: c\nupdated: u\nstate: doing\n---\nbody\n";
        let task = Task::parse(src).unwrap();
        assert_eq!(
            task.extra.get("state").and_then(serde_yaml_ng::Value::as_str),
            Some("doing")
        );
        assert_eq!(task.to_markdown(), src);
    }

    #[test]
    fn missing_opening_fence_is_an_error() {
        let err = Task::parse("title: t\n").unwrap_err();
        assert!(matches!(err, ParseError::MissingFrontmatter));
        assert!(err.to_string().contains("missing opening"));
    }

    #[test]
    fn missing_closing_fence_is_an_error() {
        let err = Task::parse("---\ntitle: t\n").unwrap_err();
        assert!(matches!(err, ParseError::UnterminatedFrontmatter));
        assert!(err.to_string().contains("unterminated"));
    }

    #[test]
    fn invalid_yaml_frontmatter_is_an_error() {
        let err = Task::parse("---\ntitle: [unterminated\n---\n").unwrap_err();
        assert!(matches!(err, ParseError::Yaml(_)));
        assert!(err.to_string().contains("invalid frontmatter"));
    }

    #[test]
    fn a_claimant_yields_claimed_even_with_an_open_blocker() {
        let mut task = Task::parse(FULL).unwrap();
        task.claimant = Some("me".into());
        assert_eq!(task.status(&unresolved), Status::Claimed);
    }

    #[test]
    fn an_unresolved_claim_blocker_yields_blocked() {
        let mut task = Task::parse(FULL).unwrap();
        task.claimant = None;
        assert_eq!(task.status(&unresolved), Status::Blocked);
    }

    #[test]
    fn a_resolved_claim_blocker_yields_ready() {
        let mut task = Task::parse(FULL).unwrap();
        task.claimant = None;
        assert_eq!(task.status(&|_| true), Status::Ready);
    }

    #[test]
    fn a_lone_close_blocker_never_blocks_status() {
        let mut task = Task::parse(FULL).unwrap();
        task.claimant = None;
        task.blockers.retain(|b| b.on == On::Close);
        assert_eq!(task.status(&unresolved), Status::Ready);
    }
}
