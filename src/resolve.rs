use crate::error::{BallError, Result};
use crate::task::{Status, Task};
use std::collections::BTreeSet;

/// Merge two task versions according to the resolution rules:
/// 1. Status: highest precedence wins
/// 2. Notes: union (dedup by ts+author+text)
/// 3. Non-status fields: later updated_at wins
/// 4. claimed_by: if resolved status is closed, from the closing side;
///    otherwise first writer (earlier updated_at)
pub fn resolve_conflict(ours: &Task, theirs: &Task) -> Task {
    // Determine winning status
    let (status_winner, other) = if ours.status.precedence() >= theirs.status.precedence() {
        (ours, theirs)
    } else {
        (theirs, ours)
    };
    let winning_status = status_winner.status;

    // Later updated_at wins for other fields
    let (newer, older) = if ours.updated_at >= theirs.updated_at {
        (ours, theirs)
    } else {
        (theirs, ours)
    };

    let mut result = newer.clone();
    result.status = winning_status;

    // Merge notes (union, dedup)
    let mut seen: BTreeSet<(String, String, String)> = BTreeSet::new();
    let mut merged_notes = Vec::new();
    for n in ours.notes.iter().chain(theirs.notes.iter()) {
        let key = (n.ts.to_rfc3339(), n.author.clone(), n.text.clone());
        if seen.insert(key) {
            merged_notes.push(n.clone());
        }
    }
    merged_notes.sort_by(|a, b| a.ts.cmp(&b.ts));
    result.notes = merged_notes;

    // claimed_by handling
    if winning_status == Status::Closed {
        result.claimed_by.clone_from(&status_winner.claimed_by);
        result.closed_at = status_winner.closed_at.or(other.closed_at);
        result.branch = status_winner.branch.clone().or_else(|| other.branch.clone());
    } else {
        // First writer wins
        result.claimed_by = older.claimed_by.clone().or_else(|| newer.claimed_by.clone());
    }

    // updated_at becomes the later of the two
    result.updated_at = newer.updated_at;

    // Tags union (preserve order from newer, then add any in older not present)
    let mut tag_set: BTreeSet<String> = newer.tags.iter().cloned().collect();
    for t in &older.tags {
        tag_set.insert(t.clone());
    }
    let mut tags: Vec<String> = newer.tags.clone();
    for t in &older.tags {
        if !tags.contains(t) {
            tags.push(t.clone());
        }
    }
    result.tags = tags;

    // depends_on union, preserving newer's order
    let mut deps = newer.depends_on.clone();
    for d in &older.depends_on {
        if !deps.contains(d) {
            deps.push(d.clone());
        }
    }
    result.depends_on = deps;

    // Links union
    let mut links = newer.links.clone();
    for l in &older.links {
        if !links.contains(l) {
            links.push(l.clone());
        }
    }
    result.links = links;

    result
}

/// Parse a file containing git merge conflict markers for a JSON task file.
/// Returns (ours, theirs) as Task objects.
pub fn parse_conflict_markers(content: &str) -> Result<(Task, Task)> {
    let mut ours = String::new();
    let mut theirs = String::new();
    let mut state = ParseState::Neither;

    for line in content.lines() {
        if line.starts_with("<<<<<<<") {
            state = ParseState::Ours;
            continue;
        }
        if line.starts_with("=======") {
            state = ParseState::Theirs;
            continue;
        }
        if line.starts_with(">>>>>>>") {
            state = ParseState::Neither;
            continue;
        }
        match state {
            ParseState::Ours => {
                ours.push_str(line);
                ours.push('\n');
            }
            ParseState::Theirs => {
                theirs.push_str(line);
                theirs.push('\n');
            }
            ParseState::Neither => {
                ours.push_str(line);
                ours.push('\n');
                theirs.push_str(line);
                theirs.push('\n');
            }
        }
    }

    let ours_task: Task = serde_json::from_str(&ours)
        .map_err(|e| BallError::Conflict(format!("could not parse 'ours' side: {e}")))?;
    let theirs_task: Task = serde_json::from_str(&theirs)
        .map_err(|e| BallError::Conflict(format!("could not parse 'theirs' side: {e}")))?;
    Ok((ours_task, theirs_task))
}

enum ParseState {
    Neither,
    Ours,
    Theirs,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{NewTaskOpts, Note, Task};
    use chrono::{Duration, Utc};

    fn base(id: &str) -> Task {
        Task::new(
            NewTaskOpts {
                title: id.into(),
                ..Default::default()
            },
            id.into(),
        )
    }

    #[test]
    fn status_precedence_closed_wins() {
        let mut ours = base("a");
        ours.status = Status::InProgress;
        let mut theirs = base("a");
        theirs.status = Status::Closed;
        theirs.claimed_by = Some("bob".into());
        let merged = resolve_conflict(&ours, &theirs);
        assert_eq!(merged.status, Status::Closed);
        assert_eq!(merged.claimed_by.as_deref(), Some("bob"));
    }

    #[test]
    fn notes_are_unioned() {
        let mut ours = base("a");
        ours.append_note("alice", "hello");
        let mut theirs = base("a");
        theirs.append_note("bob", "world");
        let merged = resolve_conflict(&ours, &theirs);
        assert_eq!(merged.notes.len(), 2);
    }

    #[test]
    fn later_updated_at_wins_for_fields() {
        let mut ours = base("a");
        ours.title = "old".into();
        ours.updated_at = Utc::now() - Duration::hours(1);
        let mut theirs = base("a");
        theirs.title = "new".into();
        theirs.updated_at = Utc::now();
        let merged = resolve_conflict(&ours, &theirs);
        assert_eq!(merged.title, "new");
    }

    #[test]
    fn duplicate_notes_are_deduped() {
        let ts = Utc::now();
        let mut ours = base("a");
        ours.notes.push(Note {
            ts,
            author: "x".into(),
            text: "dup".into(),
        });
        let mut theirs = base("a");
        theirs.notes.push(Note {
            ts,
            author: "x".into(),
            text: "dup".into(),
        });
        let merged = resolve_conflict(&ours, &theirs);
        assert_eq!(merged.notes.len(), 1);
    }

    #[test]
    fn tags_union() {
        let mut ours = base("a");
        ours.tags = vec!["x".into(), "y".into()];
        ours.updated_at = Utc::now();
        let mut theirs = base("a");
        theirs.tags = vec!["y".into(), "z".into()];
        theirs.updated_at = Utc::now() - Duration::hours(1);
        let merged = resolve_conflict(&ours, &theirs);
        assert!(merged.tags.contains(&"x".to_string()));
        assert!(merged.tags.contains(&"y".to_string()));
        assert!(merged.tags.contains(&"z".to_string()));
    }

    #[test]
    fn deps_union() {
        let mut ours = base("a");
        ours.depends_on = vec!["a".into(), "b".into()];
        ours.updated_at = Utc::now();
        let mut theirs = base("a");
        theirs.depends_on = vec!["b".into(), "c".into()];
        theirs.updated_at = Utc::now() - Duration::hours(1);
        let merged = resolve_conflict(&ours, &theirs);
        assert_eq!(merged.depends_on.len(), 3);
    }

    #[test]
    fn claimed_by_first_writer_when_not_closed() {
        let mut ours = base("a");
        ours.claimed_by = Some("alice".into());
        ours.updated_at = Utc::now() - Duration::hours(1);
        let mut theirs = base("a");
        theirs.claimed_by = Some("bob".into());
        theirs.updated_at = Utc::now();
        let merged = resolve_conflict(&ours, &theirs);
        assert_eq!(merged.claimed_by.as_deref(), Some("alice"));
    }

    #[test]
    fn parse_markers_roundtrip() {
        let t = base("a");
        let j = serde_json::to_string_pretty(&t).unwrap();
        let conflict = format!("<<<<<<< HEAD\n{j}\n=======\n{j}\n>>>>>>> theirs\n");
        let (ours, theirs) = parse_conflict_markers(&conflict).unwrap();
        assert_eq!(ours.id, "a");
        assert_eq!(theirs.id, "a");
    }

    #[test]
    fn links_union() {
        use crate::task::{Link, LinkType};
        let mut ours = base("a");
        ours.links = vec![Link { link_type: LinkType::RelatesTo, target: "x".into() }];
        ours.updated_at = Utc::now();
        let mut theirs = base("a");
        theirs.links = vec![Link { link_type: LinkType::Duplicates, target: "y".into() }];
        theirs.updated_at = Utc::now() - Duration::hours(1);
        let merged = resolve_conflict(&ours, &theirs);
        assert_eq!(merged.links.len(), 2);
    }

    #[test]
    fn parse_markers_invalid_returns_conflict_error() {
        let conflict = "<<<<<<< HEAD\nnot json\n=======\nnot json either\n>>>>>>> theirs\n";
        let err = parse_conflict_markers(conflict).unwrap_err();
        assert!(matches!(err, crate::error::BallError::Conflict(_)));
    }
}
