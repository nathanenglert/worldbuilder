//! The on-disk shape of a world.
//!
//! Deliberately small. Events do **not** carry an "effects" block that mutates other
//! records — facts own their own validity, and anchor to events by date. That leaves
//! exactly one source of truth, keeps files order-independent, and means an event can
//! be re-dated without replaying anything.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use wb_core::{Calendar, DateExpr, Fuzz};

/// The five roles the engine reasons about. User-facing type names ("Duchy", "Hive",
/// "Orbital") declare which of these they behave like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Primitive {
    /// Existence interval, parentage, held titles.
    Actor,
    /// Territory over time, membership, rise and fall.
    Polity,
    /// Geometry, founding and destruction, changes hands.
    Place,
    /// A dated occurrence other facts hang off.
    Event,
    /// No geometry; may still have intervals. Languages, magic systems, artifacts.
    Thing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeDef {
    pub name: String,
    pub primitive: Primitive,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// A fact's value. Untagged, so YAML scalars land in the obvious variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl Value {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(t),
            _ => None,
        }
    }

    /// Fact values that name another entity are written as a bare id (`pol_vashen`).
    pub fn as_ref_id(&self) -> Option<&str> {
        self.as_text().filter(|t| {
            !t.is_empty()
                && t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && t.contains('_')
        })
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(v) => write!(f, "{v}"),
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::Text(v) => write!(f, "{v}"),
        }
    }
}

/// A window an assertion holds over. Omitting either end means unbounded that way,
/// which is what lets a writer record a fact before knowing when it started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    #[serde(default = "DateExpr::unknown")]
    pub from: DateExpr,
    #[serde(default = "DateExpr::unknown")]
    pub to: DateExpr,
}

impl Default for Span {
    fn default() -> Self {
        Self { from: DateExpr::unknown(), to: DateExpr::unknown() }
    }
}

/// One time-indexed assertion about an entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub attr: String,
    pub value: Value,
    #[serde(default = "DateExpr::unknown")]
    pub from: DateExpr,
    #[serde(default = "DateExpr::unknown")]
    pub to: DateExpr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existence: Option<Span>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<Fact>,

    /// A point on the map: `[x, y]`, normalized to 0..1 with the origin top-left.
    ///
    /// Normalized rather than pixel coordinates so a backdrop can be swapped, rescaled,
    /// or replaced with a redrawn map without moving a single settlement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<[f64; 2]>,

    /// A closed polygon in the same coordinate space. Territory, coastline, a region.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shape: Vec<[f64; 2]>,

    /// Prose from the Markdown body. Never round-tripped through YAML.
    #[serde(skip)]
    pub body: String,
    #[serde(skip)]
    pub source: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    pub date: DateExpr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,

    #[serde(skip)]
    pub body: String,
    #[serde(skip)]
    pub source: PathBuf,
}

fn default_gestation() -> i64 {
    280
}

/// Knobs the consistency rules read. Per-world because the answers are per-world:
/// what counts as a second value for an attribute, and how long a pregnancy runs,
/// are both things a writer gets to decide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    /// Attributes that may legitimately hold several values at once (`member`, `tag`).
    /// Everything else is treated as single-valued, so two overlapping assertions are
    /// a contradiction.
    #[serde(default)]
    pub multi_valued: Vec<String>,
    #[serde(default = "default_gestation")]
    pub gestation_days: i64,
}

impl Default for Rules {
    fn default() -> Self {
        Self { multi_valued: Vec::new(), gestation_days: default_gestation() }
    }
}

impl Rules {
    pub fn is_multi_valued(&self, attr: &str) -> bool {
        self.multi_valued.iter().any(|a| a == attr)
    }
}

/// `world.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldDef {
    pub name: String,
    pub calendar: Calendar,
    #[serde(default)]
    pub fuzz: Fuzz,
    #[serde(default)]
    pub types: Vec<TypeDef>,
    #[serde(default)]
    pub rules: Rules,
}
