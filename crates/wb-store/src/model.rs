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

/// Stage 1 of the map pipeline, as the writer states it: here is my map, here is how to
/// read it. Everything downstream of `terrain` is derived and cached.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapSpec {
    /// The imported raster, relative to the world root.
    pub image: PathBuf,
    #[serde(default)]
    pub terrain: wb_terrain::TerrainParams,
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
    /// What else the prose calls this. `Aldric Vane` is also `Aldric`; `The Vale of
    /// Corrath` is also `the Vale`.
    ///
    /// A first-class list rather than a fact, following `parents:`. As a fact it would be
    /// multi-valued, so every world would have to remember to list `aka` under
    /// `rules.multi_valued` or watch the consistency engine call two nicknames a
    /// contradiction — a footgun dressed as a feature.
    ///
    /// Purely for finding the entity in prose. Nothing renders from it, so leaving it
    /// empty costs a writer only the mentions their book spells differently.
    #[serde(rename = "aka", default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
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

/// Where the manuscript is, and in what order it reads.
///
/// The one path in a world allowed to point outside the world folder, and it is declared
/// once rather than repeated on every scene. That is the whole reason it exists: the
/// escape is visible in `world.yaml` and reviewable in a diff, instead of hiding as a
/// `../` in each of two hundred records — and when the book moves, one line moves with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManuscriptSpec {
    /// Relative to the world root, and the only path resolved outside it.
    pub root: PathBuf,
    /// Chapter files in reading order. Empty means lexical filename order, which is
    /// right whenever a writer numbers their chapters and wrong the moment they reach
    /// `ch10` without zero-padding — hence the escape hatch.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
}

/// A scene: a stretch of the book, placed in the world.
///
/// The one record type that points *out* of the world folder. Everything about it is
/// deliberately thin — the prose is the writer's, lives in Scrivener or Obsidian or Word,
/// and is never copied here, never edited here, and never depended on for the record to
/// be valid. A scene with a broken link is still a scene; it just has nothing to read.
///
/// Scenes are their own record type rather than events carrying two extra keys. An event
/// is something that happened in the world; a scene is a place the *telling* touches it,
/// and conflating them would put the book's chapters onto the history track and change
/// what `event_count` means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scene {
    pub id: String,
    pub name: String,
    /// When it is set, in world time. The one field a scene cannot do without — a scene
    /// with no date cannot be checked against anything, which is the point of having it.
    pub date: DateExpr,
    /// Whose eyes it is seen through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pov: Option<String>,
    /// Who else is on the page. Not `participants`: an event's participants *did* the
    /// thing, whereas a scene's cast is simply who appears, which is weaker evidence and
    /// is treated as such.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub on_page: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// `ch12.md#scene-3`, relative to the manuscript root. Read-only, always.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prose: Option<String>,

    /// The file this scene was loaded from — not the prose it points at.
    #[serde(skip)]
    pub source: PathBuf,
}

/// `world.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldDef {
    pub name: String,
    pub calendar: Calendar,
    /// The map image and how to turn it into terrain. Absent is fine — a world with no
    /// map is still a world, and every reader has to cope with that anyway.
    #[serde(default)]
    pub map: Option<MapSpec>,
    /// Absent is the normal case. A world with no manuscript is not an incomplete world;
    /// it is a world whose iceberg is entirely below the waterline, which is a true and
    /// useful thing to be told.
    #[serde(default)]
    pub manuscript: Option<ManuscriptSpec>,
    #[serde(default)]
    pub fuzz: Fuzz,
    #[serde(default)]
    pub types: Vec<TypeDef>,
    #[serde(default)]
    pub rules: Rules,
}
