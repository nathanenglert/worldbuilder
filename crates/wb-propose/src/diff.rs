//! Two worlds, described in records rather than in lines.
//!
//! This is what makes a what-if branch worth having. `git diff` can already say that
//! eleven lines of `aldric-vane.md` changed; only something that has loaded both sides
//! can say *"Aldric Vane — existence, and his death moves 401 days later; two open
//! questions settled, none introduced."*
//!
//! It lives here rather than in `wb-git` for the same reason [`impact_between`] does:
//! `wb-git` extracts a subtree and knows nothing about records, and the crate that
//! already owns "compare two worlds that both exist" should own all of it.
//!
//! **A date can move without a file changing.** Re-dating the siege drags every fact
//! anchored to it, so a record can appear here with no changed fields at all and a moved
//! date — which is precisely the consequence a writer wants surfaced, and precisely the
//! one a line diff cannot show them.

use serde::Serialize;
use wb_store::{Entity, Event, Scene, World};

use crate::impact::{Impact, impact_between};

/// A record, named the way the panel needs it.
#[derive(Debug, Clone, Serialize)]
pub struct RecordRef {
    pub id: String,
    pub name: String,
    /// `entity` · `event` · `scene`.
    pub kind: &'static str,
}

/// A date node that resolves differently on the two sides.
#[derive(Debug, Clone, Serialize)]
pub struct Moved {
    /// `date` for an event or a scene, `birth` or `death` for an entity.
    pub what: &'static str,
    pub from: Option<i64>,
    pub to: Option<i64>,
    /// Positive is later. Zero when one side has no date at all, in which case the
    /// interesting part is the `from`/`to` pair rather than the distance.
    pub days: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecordChange {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    /// Field names, not line numbers: `existence`, `facts +1 −1`, `marker`.
    pub fields: Vec<String>,
    pub moved: Vec<Moved>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WorldDiff {
    pub added: Vec<RecordRef>,
    pub removed: Vec<RecordRef>,
    pub changed: Vec<RecordChange>,
    /// Which contradictions the second world settles, and which it creates.
    pub impact: Impact,
}

impl WorldDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// `(added, removed, changed)`, for a one-line summary.
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.added.len(), self.removed.len(), self.changed.len())
    }
}

/// How many of `mine` are missing from `theirs`, and the other way round.
fn tally<T: PartialEq>(before: &[T], after: &[T]) -> (usize, usize) {
    let gained = after.iter().filter(|x| !before.contains(x)).count();
    let lost = before.iter().filter(|x| !after.contains(x)).count();
    (gained, lost)
}

fn plural(label: &str, gained: usize, lost: usize) -> Option<String> {
    match (gained, lost) {
        (0, 0) => None,
        (g, 0) => Some(format!("{label} +{g}")),
        (0, l) => Some(format!("{label} −{l}")),
        (g, l) => Some(format!("{label} +{g} −{l}")),
    }
}

/// `source` is deliberately never compared: the other side of a comparison has been
/// materialized into a scratch directory, so every path differs and none of it means
/// anything about the record.
fn entity_fields(before: &Entity, after: &Entity) -> Vec<String> {
    let mut out = Vec::new();
    if before.name != after.name {
        out.push("name".to_string());
    }
    if before.type_name != after.type_name {
        out.push("type".to_string());
    }
    if before.existence != after.existence {
        out.push("existence".to_string());
    }
    let (gained, lost) = tally(&before.aliases, &after.aliases);
    if let Some(word) = plural("aka", gained, lost) {
        out.push(word);
    }
    let (gained, lost) = tally(&before.parents, &after.parents);
    if let Some(word) = plural("parents", gained, lost) {
        out.push(word);
    }
    let (gained, lost) = tally(&before.facts, &after.facts);
    if let Some(word) = plural("facts", gained, lost) {
        out.push(word);
    }
    if before.marker != after.marker {
        out.push("marker".to_string());
    }
    if before.shape != after.shape {
        out.push("shape".to_string());
    }
    if before.body.trim() != after.body.trim() {
        out.push("prose".to_string());
    }
    out
}

fn event_fields(before: &Event, after: &Event) -> Vec<String> {
    let mut out = Vec::new();
    if before.name != after.name {
        out.push("name".to_string());
    }
    if before.kind != after.kind {
        out.push("kind".to_string());
    }
    if before.date != after.date {
        out.push("date".to_string());
    }
    let (gained, lost) = tally(&before.participants, &after.participants);
    if let Some(word) = plural("participants", gained, lost) {
        out.push(word);
    }
    if before.location != after.location {
        out.push("location".to_string());
    }
    if before.body.trim() != after.body.trim() {
        out.push("prose".to_string());
    }
    out
}

fn scene_fields(before: &Scene, after: &Scene) -> Vec<String> {
    let mut out = Vec::new();
    if before.name != after.name {
        out.push("name".to_string());
    }
    if before.date != after.date {
        out.push("date".to_string());
    }
    if before.pov != after.pov {
        out.push("pov".to_string());
    }
    let (gained, lost) = tally(&before.on_page, &after.on_page);
    if let Some(word) = plural("on the page", gained, lost) {
        out.push(word);
    }
    if before.location != after.location {
        out.push("location".to_string());
    }
    if before.prose != after.prose {
        out.push("prose link".to_string());
    }
    out
}

/// Where a date node landed on each side, when the two disagree.
fn moved(before: &World, after: &World, node: &str, what: &'static str) -> Option<Moved> {
    let one = before.resolved_node(node).and_then(|r| r.nominal).map(|d| d.0);
    let two = after.resolved_node(node).and_then(|r| r.nominal).map(|d| d.0);
    if one == two {
        return None;
    }
    let days = match (one, two) {
        (Some(a), Some(b)) => b - a,
        _ => 0,
    };
    Some(Moved { what, from: one, to: two, days })
}

fn change(
    before: &World,
    after: &World,
    id: &str,
    name: &str,
    kind: &'static str,
    fields: Vec<String>,
    nodes: &[(&str, &'static str)],
) -> Option<RecordChange> {
    let moves: Vec<Moved> =
        nodes.iter().filter_map(|(node, what)| moved(before, after, node, what)).collect();
    if fields.is_empty() && moves.is_empty() {
        return None;
    }
    Some(RecordChange { id: id.to_string(), name: name.to_string(), kind, fields, moved: moves })
}

/// Everything that is different between two loaded worlds.
///
/// Both sides are read at their own root, so this works equally on "the working tree
/// against a commit" and "one commit against another". The consistency half comes
/// straight from [`impact_between`], so a comparison and a proposal review report
/// findings in exactly the same words.
pub fn diff_worlds(before: &World, after: &World) -> WorldDiff {
    let mut out = WorldDiff { impact: impact_between(before, after), ..WorldDiff::default() };

    for (id, entity) in &before.entities {
        match after.entities.get(id) {
            None => out.removed.push(RecordRef {
                id: id.clone(),
                name: entity.name.clone(),
                kind: "entity",
            }),
            Some(now) => {
                let birth = format!("{id}.birth");
                let death = format!("{id}.death");
                if let Some(c) = change(
                    before,
                    after,
                    id,
                    &now.name,
                    "entity",
                    entity_fields(entity, now),
                    &[(&birth, "birth"), (&death, "death")],
                ) {
                    out.changed.push(c);
                }
            }
        }
    }
    for (id, entity) in &after.entities {
        if !before.entities.contains_key(id) {
            out.added.push(RecordRef { id: id.clone(), name: entity.name.clone(), kind: "entity" });
        }
    }

    for (id, event) in &before.events {
        match after.events.get(id) {
            None => out.removed.push(RecordRef {
                id: id.clone(),
                name: event.name.clone(),
                kind: "event",
            }),
            Some(now) => {
                if let Some(c) = change(
                    before,
                    after,
                    id,
                    &now.name,
                    "event",
                    event_fields(event, now),
                    &[(id, "date")],
                ) {
                    out.changed.push(c);
                }
            }
        }
    }
    for (id, event) in &after.events {
        if !before.events.contains_key(id) {
            out.added.push(RecordRef { id: id.clone(), name: event.name.clone(), kind: "event" });
        }
    }

    for (id, scene) in &before.scenes {
        match after.scenes.get(id) {
            None => out.removed.push(RecordRef {
                id: id.clone(),
                name: scene.name.clone(),
                kind: "scene",
            }),
            Some(now) => {
                if let Some(c) = change(
                    before,
                    after,
                    id,
                    &now.name,
                    "scene",
                    scene_fields(scene, now),
                    &[(id, "date")],
                ) {
                    out.changed.push(c);
                }
            }
        }
    }
    for (id, scene) in &after.scenes {
        if !before.scenes.contains_key(id) {
            out.added.push(RecordRef { id: id.clone(), name: scene.name.clone(), kind: "scene" });
        }
    }

    out
}
