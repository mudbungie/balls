use crate::error::{BallError, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Task lifecycle status.
///
/// `Unknown(String)` exists purely for forward compatibility, mirroring
/// `LinkType::Unknown`: if a newer `bl` writes a status we don't recognize,
/// older clients round-trip it verbatim instead of hard-erroring on the
/// whole task file. `Status::parse` (the CLI entry point) never produces
/// `Unknown` — users cannot craft one by hand. An `Unknown` status has the
/// lowest precedence, so conflict resolution never accidentally elects it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Open,
    InProgress,
    Review,
    Blocked,
    Closed,
    Deferred,
    Unknown(String),
}

impl Status {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "open" => Ok(Status::Open),
            "in_progress" => Ok(Status::InProgress),
            "review" => Ok(Status::Review),
            "blocked" => Ok(Status::Blocked),
            "closed" => Ok(Status::Closed),
            "deferred" => Ok(Status::Deferred),
            _ => Err(BallError::InvalidTask(format!("unknown status: {s}"))),
        }
    }

    pub fn precedence(&self) -> u8 {
        match self {
            Status::Closed => 6,
            Status::Review => 5,
            Status::InProgress => 4,
            Status::Blocked => 3,
            Status::Open => 2,
            Status::Deferred => 1,
            Status::Unknown(_) => 0,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Status::Open => "open",
            Status::InProgress => "in_progress",
            Status::Review => "review",
            Status::Blocked => "blocked",
            Status::Closed => "closed",
            Status::Deferred => "deferred",
            Status::Unknown(s) => s.as_str(),
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Status {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "open" => Status::Open,
            "in_progress" => Status::InProgress,
            "review" => Status::Review,
            "blocked" => Status::Blocked,
            "closed" => Status::Closed,
            "deferred" => Status::Deferred,
            _ => Status::Unknown(s),
        })
    }
}
