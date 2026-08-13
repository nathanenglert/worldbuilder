//! What an agent sees.
//!
//! These are not the internal types with `Serialize` bolted on. An agent reading this
//! world has no screen, no scrubber, and no way to click through to a record — so every
//! payload carries the resolved answer *and* the expression it came from, names
//! alongside ids, and readable date labels alongside day numbers. The cost is some
//! redundancy on the wire; the benefit is that a model never has to guess.
//!
//! Uncertainty is never flattened. `certainty: "maybe"` means the world's own fuzzy
//! dates leave the question genuinely open, and an agent reporting that as settled is
//! inventing canon.

use schemars::JsonSchema;
use serde::Serialize;
use wb_core::{Calendar, Containment, DateExpr, FuzzyInterval, Resolved};
use wb_store::{Entity, EntityView, Event, Hit, Primitive, World};

pub fn certainty(c: Containment) -> &'static str {
    match c {
        Containment::Yes => "yes",
        Containment::Maybe => "maybe",
        Containment::No => "no",
    }
}

pub fn primitive_name(p: Primitive) -> &'static str {
    match p {
        Primitive::Actor => "actor",
        Primitive::Polity => "polity",
        Primitive::Place => "place",
        Primitive::Event => "event",
        Primitive::Thing => "thing",
    }
}

/// Paths are reported relative to the world root: an absolute path leaks the writer's
/// home directory into a transcript and is longer for no gain.
pub fn relative(world: &World, path: &std::path::Path) -> String {
    path.strip_prefix(&world.root).unwrap_or(path).display().to_string()
}

/// A date, as authored and as resolved. Both halves matter — the expression is what a
/// proposal would have to edit, the resolution is what every comparison runs against.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DateDto {
    /// Exactly as written in the file: `0812~`, `@evt_siege_of_marrow+1y`, `?`.
    pub expr: String,
    pub label: String,
    /// The canonical day scalar. `null` when the date is unknown or open-ended.
    pub day: Option<i64>,
    /// How wide the doubt is. Both equal `day` when the date is exact.
    pub earliest: Option<i64>,
    pub latest: Option<i64>,
    pub exact: bool,
}

impl DateDto {
    pub fn new(cal: &Calendar, expr: String, r: Resolved) -> Self {
        let label = match r.nominal {
            Some(day) => cal.format_long(day),
            None if expr == "?" || expr.is_empty() => "unknown".to_string(),
            None => "open".to_string(),
        };
        Self {
            label,
            day: r.nominal.map(|d| d.0),
            earliest: r.earliest.map(|d| d.0),
            latest: r.latest.map(|d| d.0),
            exact: r.is_exact(),
            expr,
        }
    }

    /// One endpoint of a fact or lifespan, resolved in its owner's context.
    pub fn endpoint(world: &World, owner: &str, expr: &DateExpr) -> Self {
        let resolved = world.resolve_in(owner, expr).unwrap_or_else(|_| Resolved::unknown());
        Self::new(&world.calendar, expr.to_string(), resolved)
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SpanDto {
    pub from: DateDto,
    pub to: DateDto,
}

impl SpanDto {
    pub fn of(world: &World, owner: &str, span: &wb_store::Span) -> Self {
        Self {
            from: DateDto::endpoint(world, owner, &span.from),
            to: DateDto::endpoint(world, owner, &span.to),
        }
    }
}

/// A fact with the window it holds over — the full record, not a moment.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FactDto {
    pub attr: String,
    pub value: String,
    /// Set when the value names another record, so a reference can be followed without
    /// guessing whether `place_marrow` is an id or a string that happens to look like one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references: Option<String>,
    pub from: DateDto,
    pub to: DateDto,
}

impl FactDto {
    pub fn of(world: &World, owner: &str, fact: &wb_store::Fact) -> Self {
        Self {
            attr: fact.attr.clone(),
            value: fact.value.to_string(),
            references: reference(world, &fact.value),
            from: DateDto::endpoint(world, owner, &fact.from),
            to: DateDto::endpoint(world, owner, &fact.to),
        }
    }
}

/// A fact as it stands at one instant. Leaner than [`FactDto`] on purpose: a snapshot
/// answers "what is true now", and the day numbers are what an agent compares against.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LiveFactDto {
    pub attr: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references: Option<String>,
    /// `yes` if this holds under every reading of the world's fuzzy dates, `maybe` if
    /// the vagueness leaves it open. `maybe` is a fact about the world, not a defect.
    pub certainty: &'static str,
    /// The outer window `[since, until)` this fact could hold over. `null` is unbounded.
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub window: String,
}

impl LiveFactDto {
    pub fn of(world: &World, fact: &wb_store::FactAt<'_>) -> Self {
        Self {
            attr: fact.attr.to_string(),
            value: fact.value.to_string(),
            references: reference(world, fact.value),
            certainty: certainty(fact.certainty),
            since: fact.span.possible.from.map(|d| d.0),
            until: fact.span.possible.to.map(|d| d.0),
            window: window_label(&world.calendar, &fact.span),
        }
    }
}

fn reference(world: &World, value: &wb_store::Value) -> Option<String> {
    value.as_ref_id().filter(|id| world.knows(id)).map(str::to_string)
}

fn window_label(cal: &Calendar, span: &FuzzyInterval) -> String {
    match (span.possible.from, span.possible.to) {
        (None, None) => "always".to_string(),
        (Some(a), None) => format!("since {}", cal.format_numeric(a)),
        (None, Some(b)) => format!("until {}", cal.format_numeric(b)),
        (Some(a), Some(b)) => {
            format!("{} → {}", cal.format_numeric(a), cal.format_numeric(b))
        }
    }
}

/// An entity in a list — enough to decide whether to fetch the whole record.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EntityBrief {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub primitive: Option<&'static str>,
    /// `yes` or `maybe`, and only when the brief was asked for at a date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existence: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<LiveFactDto>,
    pub has_shape: bool,
    pub has_marker: bool,
}

impl EntityBrief {
    pub fn of(world: &World, entity: &Entity) -> Self {
        Self {
            id: entity.id.clone(),
            name: entity.name.clone(),
            type_name: entity.type_name.clone(),
            primitive: world.primitive_of(entity).map(primitive_name),
            existence: None,
            facts: Vec::new(),
            has_shape: !entity.shape.is_empty(),
            has_marker: entity.marker.is_some(),
        }
    }

    /// The same brief as it stood at one instant, carrying only the facts live then.
    pub fn at(world: &World, view: &EntityView<'_>) -> Self {
        Self {
            existence: Some(certainty(view.existence)),
            facts: view.facts.iter().map(|f| LiveFactDto::of(world, f)).collect(),
            ..Self::of(world, view.entity)
        }
    }
}

/// A whole record. Includes what points *at* it, which no file states and every
/// question about a person or a place turns out to need.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EntityDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub primitive: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existence: Option<SpanDto>,
    pub parents: Vec<NamedRef>,
    pub children: Vec<NamedRef>,
    pub facts: Vec<FactDto>,
    /// Present only when the record was asked for at a date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<AtDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker: Option<[f64; 2]>,
    pub shape_points: usize,
    /// Events naming this record as a participant or a location.
    pub appears_in: Vec<NamedRef>,
    /// Other records whose facts point here.
    pub referenced_by: Vec<NamedRef>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub body: String,
    pub source: String,
}

/// The part of a record that depends on when you asked.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AtDto {
    pub day: i64,
    pub label: String,
    /// `no` means the record did not exist then — the map would not draw it at all.
    pub existence: &'static str,
    pub facts: Vec<LiveFactDto>,
}

impl EntityDto {
    pub fn of(world: &World, entity: &Entity, at: Option<wb_core::Day>) -> Self {
        let id = entity.id.as_str();

        let children: Vec<NamedRef> = world
            .entities
            .values()
            .filter(|e| e.parents.iter().any(|p| p == id))
            .map(|e| NamedRef::known(&e.id, &e.name))
            .collect();

        let appears_in: Vec<NamedRef> = world
            .events
            .values()
            .filter(|e| e.participants.iter().any(|p| p == id) || e.location.as_deref() == Some(id))
            .map(|e| NamedRef::known(&e.id, &e.name))
            .collect();

        let referenced_by: Vec<NamedRef> = world
            .entities
            .values()
            .filter(|e| e.id != id && e.facts.iter().any(|f| f.value.as_ref_id() == Some(id)))
            .map(|e| NamedRef::known(&e.id, &e.name))
            .collect();

        let at = at.and_then(|day| {
            let view = world.entity_at(id, day)?;
            Some(AtDto {
                day: day.0,
                label: world.calendar.format_long(day),
                existence: certainty(view.existence),
                facts: view.facts.iter().map(|f| LiveFactDto::of(world, f)).collect(),
            })
        });

        Self {
            id: entity.id.clone(),
            name: entity.name.clone(),
            type_name: entity.type_name.clone(),
            primitive: world.primitive_of(entity).map(primitive_name),
            existence: entity.existence.as_ref().map(|s| SpanDto::of(world, id, s)),
            parents: entity.parents.iter().map(|p| NamedRef::of(world, p)).collect(),
            children,
            facts: entity.facts.iter().map(|f| FactDto::of(world, id, f)).collect(),
            at,
            marker: entity.marker,
            shape_points: entity.shape.len(),
            appears_in,
            referenced_by,
            body: entity.body.trim().to_string(),
            source: relative(world, &entity.source),
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EventDto {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kind: String,
    pub date: DateDto,
    pub participants: Vec<NamedRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<NamedRef>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub body: String,
    pub source: String,
}

impl EventDto {
    pub fn of(world: &World, event: &Event) -> Self {
        let resolved = world.resolved_node(&event.id).unwrap_or_else(Resolved::unknown);
        Self {
            id: event.id.clone(),
            name: event.name.clone(),
            kind: event.kind.clone(),
            date: DateDto::new(&world.calendar, event.date.to_string(), resolved),
            participants: event.participants.iter().map(|id| NamedRef::of(world, id)).collect(),
            location: event.location.as_ref().map(|id| NamedRef::of(world, id)),
            body: event.body.trim().to_string(),
            source: relative(world, &event.source),
        }
    }
}

/// An id with its name attached. Nothing in a payload requires a second lookup, and a
/// dangling reference says so rather than looking like a valid one.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct NamedRef {
    pub id: String,
    pub name: String,
    /// True when nothing in the world defines this id.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
}

impl NamedRef {
    pub fn known(id: &str, name: &str) -> Self {
        Self { id: id.to_string(), name: name.to_string(), missing: false }
    }

    pub fn of(world: &World, id: &str) -> Self {
        if let Some(entity) = world.entities.get(id) {
            return Self::known(id, &entity.name);
        }
        if let Some(event) = world.events.get(id) {
            return Self::known(id, &event.name);
        }
        Self { id: id.to_string(), name: String::new(), missing: true }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FindingDto {
    pub rule: &'static str,
    pub title: &'static str,
    /// `definite` is wrong under every reading of every fuzzy date. `possible` is the
    /// shape a deliberate mystery takes — judge it, do not assume it is a bug.
    pub certainty: &'static str,
    pub subject: String,
    pub related: Vec<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_label: Option<String>,
    pub sources: Vec<String>,
}

impl FindingDto {
    pub fn of(world: &World, finding: &wb_check::Finding) -> Self {
        Self {
            rule: finding.rule.slug(),
            title: finding.rule.title(),
            certainty: finding.certainty.slug(),
            subject: finding.subject.clone(),
            related: finding.related.clone(),
            message: finding.message.clone(),
            at: finding.at.map(|d| d.0),
            at_label: finding.at.map(|d| world.calendar.format_long(d)),
            sources: finding.sources.iter().map(|p| relative(world, p)).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct HitDto {
    pub id: String,
    pub name: String,
    /// The declared type for an entity, the event kind for an event.
    pub kind: String,
    pub is_event: bool,
    /// Which field earned the hit: `name`, `id`, `type`, `fact`, or `prose`.
    pub matched: &'static str,
    pub excerpt: String,
}

impl HitDto {
    pub fn of(hit: &Hit<'_>) -> Self {
        Self {
            id: hit.id.to_string(),
            name: hit.name.to_string(),
            kind: hit.kind.to_string(),
            is_event: hit.is_event,
            matched: hit.matched,
            excerpt: hit.excerpt.clone(),
        }
    }
}
