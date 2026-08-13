//! What a proposed change looks like on disk.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use wb_core::DateExpr;
use wb_store::{Fact, Span, Value};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Pending,
    Accepted,
    Rejected,
}

impl Status {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }
}

/// One edit. Deliberately granular rather than a whole-file replacement: a reviewer
/// should be able to read what is being asked for without diffing YAML in their head,
/// and an agent should not be able to smuggle an unrelated change into a big blob.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Change {
    CreateEntity {
        id: String,
        name: String,
        #[serde(rename = "type")]
        type_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        existence: Option<Span>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        parents: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        facts: Vec<Fact>,
    },
    CreateEvent {
        id: String,
        name: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        kind: String,
        date: DateExpr,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        participants: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        location: Option<String>,
    },
    AddFact {
        entity: String,
        attr: String,
        value: Value,
        #[serde(default = "DateExpr::unknown")]
        from: DateExpr,
        #[serde(default = "DateExpr::unknown")]
        to: DateExpr,
    },
    /// Matched on attribute and value together, since an entity may carry the same
    /// attribute several times over different windows.
    RemoveFact {
        entity: String,
        attr: String,
        value: Value,
    },
    SetExistence {
        entity: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<DateExpr>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<DateExpr>,
    },
    SetEventDate {
        event: String,
        date: DateExpr,
    },
}

impl Change {
    /// The record this change writes to.
    pub fn target(&self) -> &str {
        match self {
            Self::CreateEntity { id, .. } | Self::CreateEvent { id, .. } => id,
            Self::AddFact { entity, .. }
            | Self::RemoveFact { entity, .. }
            | Self::SetExistence { entity, .. } => entity,
            Self::SetEventDate { event, .. } => event,
        }
    }

    /// A one-line description for a reviewer.
    pub fn summary(&self) -> String {
        match self {
            Self::CreateEntity { name, type_name, .. } => {
                format!("create {type_name} “{name}”")
            }
            Self::CreateEvent { name, date, .. } => format!("create event “{name}” at {date}"),
            Self::AddFact { entity, attr, value, from, to } => {
                format!("{entity}: add {attr} = {value} ({from} → {to})")
            }
            Self::RemoveFact { entity, attr, value } => {
                format!("{entity}: remove {attr} = {value}")
            }
            Self::SetExistence { entity, from, to } => {
                let part = |label: &str, d: &Option<DateExpr>| {
                    d.as_ref().map(|d| format!("{label} {d}")).unwrap_or_default()
                };
                format!("{entity}: existence {} {}", part("from", from), part("to", to))
                    .trim_end()
                    .to_string()
            }
            Self::SetEventDate { event, date } => format!("{event}: date → {date}"),
        }
    }
}

/// A set of changes awaiting a human decision.
///
/// Nothing an agent writes reaches canon directly. That costs nothing extra, because a
/// novelist wants canon-versus-speculative staging anyway — the review queue is the
/// same mechanism serving both.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub title: String,
    /// Who or what asked for this — an agent name, or a person.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default)]
    pub status: Status,
    pub changes: Vec<Change>,

    #[serde(skip)]
    pub source: PathBuf,
}

impl Proposal {
    pub fn is_pending(&self) -> bool {
        self.status == Status::Pending
    }
}
