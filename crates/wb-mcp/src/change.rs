//! The change vocabulary an agent writes in.
//!
//! Deliberately a mirror of [`wb_propose::Change`] rather than the type itself. Three
//! reasons, all about what a model can actually get right:
//!
//! - **Dates arrive as plain strings** and are parsed here, so a malformed one produces
//!   `bad date "0812~~": unexpected '~'` instead of a serde error naming a field.
//! - **The schema stays shallow.** `create_entity` takes `existence_from` and
//!   `existence_to` rather than a nested object, because nested optional structs are
//!   exactly where a model drops a level.
//! - **The on-disk format stays free to change** without changing the agent contract.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use wb_core::parse_date;
use wb_propose::Change;
use wb_store::{Fact, Span, Value};

/// A fact value. Numbers stay numbers and booleans stay booleans, because a population
/// written as `"9000"` sorts and compares as text forever after.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ValueInput {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl From<ValueInput> for Value {
    fn from(v: ValueInput) -> Self {
        match v {
            ValueInput::Bool(b) => Value::Bool(b),
            ValueInput::Int(i) => Value::Int(i),
            ValueInput::Float(f) => Value::Float(f),
            ValueInput::Text(t) => Value::Text(t),
        }
    }
}

/// One time-indexed assertion. Omitting `from` or `to` leaves that end open, which is
/// how a fact gets recorded before anyone knows when it started.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FactInput {
    /// The attribute name: `owner`, `title`, `population`, `capital`.
    pub attr: String,
    pub value: ValueInput,
    /// Date expression the fact starts holding at. Omit for "always was".
    #[serde(default)]
    pub from: Option<String>,
    /// Date expression it stops holding at. Omit for "still does".
    #[serde(default)]
    pub to: Option<String>,
}

/// One proposed edit.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ChangeInput {
    /// Add a record that does not exist yet.
    CreateEntity {
        /// A new id. Convention in this world is a primitive prefix: `act_`, `pol_`,
        /// `place_`, `thing_`.
        id: String,
        name: String,
        /// A type declared in `world.yaml`. An undeclared one still loads, but is
        /// reported as probably a typo.
        #[serde(rename = "type")]
        type_name: String,
        /// When it came into being. Omit for something with no meaningful start.
        #[serde(default)]
        existence_from: Option<String>,
        /// When it ceased. Omit if it still exists or nobody knows.
        #[serde(default)]
        existence_to: Option<String>,
        /// Parent ids, for lineage. Actors only, in practice.
        #[serde(default)]
        parents: Vec<String>,
        #[serde(default)]
        facts: Vec<FactInput>,
    },
    /// Add a dated occurrence. Events carry no effects — facts anchor to them by date.
    CreateEvent {
        id: String,
        name: String,
        /// Free text: `battle`, `coronation`, `treaty`. Used for grouping only.
        #[serde(default)]
        kind: Option<String>,
        date: String,
        /// Ids of entities present. Checked against their lifespans.
        #[serde(default)]
        participants: Vec<String>,
        #[serde(default)]
        location: Option<String>,
    },
    /// Add a fact to an existing record.
    AddFact {
        entity: String,
        attr: String,
        value: ValueInput,
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        to: Option<String>,
    },
    /// Remove a fact. Matched on attribute *and* value, since one attribute may be
    /// asserted several times over different windows.
    RemoveFact { entity: String, attr: String, value: ValueInput },
    /// Change when a record existed. An end you omit is **left as it was**; send `"?"`
    /// to clear one. Correcting a death date must not quietly erase the birth date, and
    /// omission is the easiest mistake to make from this side of the wire.
    SetExistence {
        entity: String,
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        to: Option<String>,
    },
    /// Move an event. Everything anchored to it moves with it.
    SetEventDate { event: String, date: String },
}

impl ChangeInput {
    pub fn into_change(self) -> Result<Change, String> {
        Ok(match self {
            Self::CreateEntity {
                id,
                name,
                type_name,
                existence_from,
                existence_to,
                parents,
                facts,
            } => {
                let existence = match (&existence_from, &existence_to) {
                    (None, None) => None,
                    _ => Some(Span {
                        from: date(existence_from.as_deref(), "existence_from")?,
                        to: date(existence_to.as_deref(), "existence_to")?,
                    }),
                };
                Change::CreateEntity {
                    id,
                    name,
                    type_name,
                    existence,
                    parents,
                    facts: facts.into_iter().map(fact).collect::<Result<_, _>>()?,
                }
            }
            Self::CreateEvent { id, name, kind, date: at, participants, location } => {
                Change::CreateEvent {
                    id,
                    name,
                    kind: kind.unwrap_or_default(),
                    date: required(&at, "date")?,
                    participants,
                    location,
                }
            }
            Self::AddFact { entity, attr, value, from, to } => Change::AddFact {
                entity,
                attr,
                value: value.into(),
                from: date(from.as_deref(), "from")?,
                to: date(to.as_deref(), "to")?,
            },
            Self::RemoveFact { entity, attr, value } => {
                Change::RemoveFact { entity, attr, value: value.into() }
            }
            // `None` all the way down: the applier reads a missing end as "leave it
            // alone", and wrapping it in `Some(Unknown)` here would turn every omission
            // into a deletion.
            Self::SetExistence { entity, from, to } => Change::SetExistence {
                entity,
                from: from.as_deref().map(|d| date(Some(d), "from")).transpose()?,
                to: to.as_deref().map(|d| date(Some(d), "to")).transpose()?,
            },
            Self::SetEventDate { event, date: at } => {
                Change::SetEventDate { event, date: required(&at, "date")? }
            }
        })
    }
}

fn fact(f: FactInput) -> Result<Fact, String> {
    Ok(Fact {
        attr: f.attr,
        value: f.value.into(),
        from: date(f.from.as_deref(), "from")?,
        to: date(f.to.as_deref(), "to")?,
    })
}

/// An absent date is `?` — unknown — not an error. That is what makes the model usable
/// bottom-up: a writer records that a duchy exists long before knowing when it started.
fn date(expr: Option<&str>, field: &str) -> Result<wb_core::DateExpr, String> {
    match expr.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(wb_core::DateExpr::unknown()),
        Some(text) => parse_date(text).map_err(|e| format!("bad `{field}` date {text:?}: {e}")),
    }
}

fn required(expr: &str, field: &str) -> Result<wb_core::DateExpr, String> {
    parse_date(expr.trim()).map_err(|e| format!("bad `{field}` date {expr:?}: {e}"))
}
