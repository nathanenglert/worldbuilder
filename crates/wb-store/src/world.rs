//! An assembled world, and the time-indexed queries the map and timeline run against.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use wb_core::{
    Calendar, Containment, DateExpr, Day, Fuzz, FuzzyInterval, Interval, NodeMap, Resolved,
    Resolver, change_points, parse_date,
};

use crate::error::{Error, Result};
use crate::model::{
    Entity, Event, Fact, ManuscriptSpec, MapSpec, Primitive, Rules, Scene, TypeDef, Value, WorldDef,
};

/// One record naming another, and the way it names it.
///
/// `how` is a short phrase for a human — "participant", "fact anchor" — because the only
/// caller is a confirmation asking whether removing something is really what was meant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub by: String,
    pub how: &'static str,
}

/// One fact, as it stands at a particular moment.
#[derive(Debug, Clone, Copy)]
pub struct FactAt<'a> {
    pub attr: &'a str,
    pub value: &'a Value,
    /// `Maybe` when fuzzy dates leave it genuinely open — the dashed-border case.
    pub certainty: Containment,
    pub span: FuzzyInterval,
}

#[derive(Debug, Clone)]
pub struct EntityView<'a> {
    pub entity: &'a Entity,
    pub existence: Containment,
    pub facts: Vec<FactAt<'a>>,
}

impl<'a> EntityView<'a> {
    pub fn fact(&self, attr: &str) -> Option<&FactAt<'a>> {
        self.facts.iter().find(|f| f.attr == attr)
    }
}

/// Everything true at one instant — what the map draws.
#[derive(Debug, Clone)]
pub struct Snapshot<'a> {
    pub day: Day,
    pub entities: Vec<EntityView<'a>>,
}

impl<'a> Snapshot<'a> {
    pub fn get(&self, id: &str) -> Option<&EntityView<'a>> {
        self.entities.iter().find(|v| v.entity.id == id)
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

#[derive(Debug)]
pub struct World {
    pub root: PathBuf,
    pub name: String,
    pub calendar: Calendar,
    pub fuzz: Fuzz,
    pub rules: Rules,
    /// The map image and its pipeline settings, if this world has one.
    pub map: Option<MapSpec>,
    /// Where the book is, if this world has one attached.
    pub manuscript: Option<ManuscriptSpec>,
    pub types: BTreeMap<String, TypeDef>,
    pub entities: BTreeMap<String, Entity>,
    pub events: BTreeMap<String, Event>,
    /// Keyed the same way, in the same shared id namespace.
    pub scenes: BTreeMap<String, Scene>,

    resolved: BTreeMap<String, Resolved>,
    lifespans: BTreeMap<String, FuzzyInterval>,
    fact_spans: BTreeMap<String, Vec<FuzzyInterval>>,
    change_points: Vec<Day>,
    undeclared_types: BTreeMap<String, Vec<String>>,
}

impl World {
    /// Resolve every date in the world together and index the results.
    pub fn assemble(
        root: PathBuf,
        def: WorldDef,
        entities: Vec<Entity>,
        events: Vec<Event>,
        scenes: Vec<Scene>,
    ) -> Result<Self> {
        def.calendar.validate()?;

        let mut entity_map: BTreeMap<String, Entity> = BTreeMap::new();
        for e in entities {
            if let Some(prev) = entity_map.get(&e.id) {
                return Err(Error::DuplicateId {
                    id: e.id.clone(),
                    first: prev.source.clone(),
                    second: e.source.clone(),
                });
            }
            entity_map.insert(e.id.clone(), e);
        }

        let mut event_map: BTreeMap<String, Event> = BTreeMap::new();
        for ev in events {
            // Events and entities share one id namespace, so anchors are unambiguous.
            let clash = event_map
                .get(&ev.id)
                .map(|p| p.source.clone())
                .or_else(|| entity_map.get(&ev.id).map(|p| p.source.clone()));
            if let Some(first) = clash {
                return Err(Error::DuplicateId {
                    id: ev.id.clone(),
                    first,
                    second: ev.source.clone(),
                });
            }
            event_map.insert(ev.id.clone(), ev);
        }

        let mut scene_map: BTreeMap<String, Scene> = BTreeMap::new();
        for sc in scenes {
            let clash = scene_map
                .get(&sc.id)
                .map(|p| p.source.clone())
                .or_else(|| entity_map.get(&sc.id).map(|p| p.source.clone()))
                .or_else(|| event_map.get(&sc.id).map(|p| p.source.clone()));
            if let Some(first) = clash {
                return Err(Error::DuplicateId {
                    id: sc.id.clone(),
                    first,
                    second: sc.source.clone(),
                });
            }
            scene_map.insert(sc.id.clone(), sc);
        }

        let types: BTreeMap<String, TypeDef> =
            def.types.into_iter().map(|t| (t.name.clone(), t)).collect();

        // An undeclared type is far more often a typo than a mistake worth blocking on,
        // so it is reported, not rejected. Refusing to load would punish exactly the
        // bottom-up writer this tool is meant to accommodate.
        let mut undeclared_types: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for e in entity_map.values() {
            if !types.contains_key(&e.type_name) {
                undeclared_types.entry(e.type_name.clone()).or_default().push(e.id.clone());
            }
        }

        let mut nodes = NodeMap::new();
        for ev in event_map.values() {
            nodes.insert(ev.id.clone(), ev.date.clone());
        }
        // Scenes register as date nodes exactly as events do, which makes them anchorable:
        // `from: "@scn_ch12_s03"` dates a fact from the scene where it happens. That falls
        // out of §3.3's model at no cost, and re-dating a scene drags it along.
        for sc in scene_map.values() {
            nodes.insert(sc.id.clone(), sc.date.clone());
        }
        for e in entity_map.values() {
            let Some(span) = &e.existence else { continue };
            // Both vocabularies resolve: `.birth`/`.death` reads right for a person,
            // `.start`/`.end` for a kingdom, a language, or a trade route.
            for open in ["birth", "start"] {
                nodes.insert(format!("{}.{open}", e.id), span.from.clone());
            }
            for close in ["death", "end"] {
                nodes.insert(format!("{}.{close}", e.id), span.to.clone());
            }
        }

        let resolver = Resolver::new(&def.calendar).with_fuzz(def.fuzz);
        let resolved = resolver.resolve_all(&nodes)?;

        let mut lifespans = BTreeMap::new();
        let mut fact_spans = BTreeMap::new();
        let mut spans_seen: Vec<Interval> = Vec::new();

        for e in entity_map.values() {
            let life = match e.existence {
                // An entity with no stated existence simply always has: a language or a
                // mountain range need not be given dates to be usable.
                None => FuzzyInterval::new(&Resolved::unknown(), &Resolved::unknown()),
                Some(_) => FuzzyInterval::new(
                    &resolved[&format!("{}.start", e.id)],
                    &resolved[&format!("{}.end", e.id)],
                ),
            };
            spans_seen.extend([life.certain, life.possible]);
            lifespans.insert(e.id.clone(), life);

            let mut spans = Vec::with_capacity(e.facts.len());
            for f in &e.facts {
                let span = FuzzyInterval::new(
                    &resolver.resolve_in(&e.id, &f.from, &resolved)?,
                    &resolver.resolve_in(&e.id, &f.to, &resolved)?,
                );
                spans_seen.extend([span.certain, span.possible]);
                spans.push(span);
            }
            fact_spans.insert(e.id.clone(), spans);
        }

        for ev in event_map.values() {
            let r = resolved[&ev.id];
            spans_seen.push(Interval::inclusive(r.earliest, r.latest));
        }

        // Scene dates are change points too. A scrubber that could not stop on the day a
        // chapter happens would be the one place the story is invisible on the timeline.
        for sc in scene_map.values() {
            let r = resolved[&sc.id];
            spans_seen.push(Interval::inclusive(r.earliest, r.latest));
        }

        let change_points = change_points(spans_seen);

        Ok(Self {
            root,
            name: def.name,
            calendar: def.calendar,
            fuzz: def.fuzz,
            rules: def.rules,
            map: def.map,
            manuscript: def.manuscript,
            types,
            entities: entity_map,
            events: event_map,
            scenes: scene_map,
            resolved,
            lifespans,
            fact_spans,
            change_points,
            undeclared_types,
        })
    }

    /// Reconstruct the `world.yaml` definition, so a modified copy of this world can be
    /// reassembled without touching disk. Proposals are simulated that way.
    pub fn definition(&self) -> WorldDef {
        WorldDef {
            name: self.name.clone(),
            calendar: self.calendar.clone(),
            fuzz: self.fuzz,
            map: self.map.clone(),
            manuscript: self.manuscript.clone(),
            types: self.types.values().cloned().collect(),
            rules: self.rules.clone(),
        }
    }

    /// This world with one record replaced, or added if the id is new.
    ///
    /// Every edit goes through here rather than through field validators, because
    /// reassembly *is* the validation: duplicate ids across the shared entity/event
    /// namespace, anchor cycles, anchors pointing at a date that no longer exists, and
    /// calendar validity are all enforced by `assemble` and by nothing else. A checker
    /// written alongside it would be a second implementation to keep in agreement.
    pub fn with_entity(&self, entity: Entity) -> Result<World> {
        let mut entities = self.entities.clone();
        entities.insert(entity.id.clone(), entity);
        self.reassemble(entities, self.events.clone(), self.scenes.clone())
    }

    pub fn with_event(&self, event: Event) -> Result<World> {
        let mut events = self.events.clone();
        events.insert(event.id.clone(), event);
        self.reassemble(self.entities.clone(), events, self.scenes.clone())
    }

    pub fn with_scene(&self, scene: Scene) -> Result<World> {
        let mut scenes = self.scenes.clone();
        scenes.insert(scene.id.clone(), scene);
        self.reassemble(self.entities.clone(), self.events.clone(), scenes)
    }

    /// This world without `id`, whatever kind of record it names.
    ///
    /// Fails with an unresolvable-anchor error if anything still dates itself against the
    /// record being removed, which is the answer a delete confirmation wants.
    pub fn without(&self, id: &str) -> Result<World> {
        let mut entities = self.entities.clone();
        let mut events = self.events.clone();
        let mut scenes = self.scenes.clone();
        entities.remove(id);
        events.remove(id);
        scenes.remove(id);
        self.reassemble(entities, events, scenes)
    }

    fn reassemble(
        &self,
        entities: BTreeMap<String, Entity>,
        events: BTreeMap<String, Event>,
        scenes: BTreeMap<String, Scene>,
    ) -> Result<World> {
        World::assemble(
            self.root.clone(),
            self.definition(),
            entities.into_values().collect(),
            events.into_values().collect(),
            scenes.into_values().collect(),
        )
    }

    /// Everything that names `id`, in the four ways a record can be named.
    ///
    /// Anchors count, and they are the ones a writer forgets: `@evt_siege_of_marrow`
    /// dates a fact somewhere else entirely, and `act_aldric.death` reaches an entity
    /// through a suffix the id itself never carries.
    pub fn references_to(&self, id: &str) -> Vec<Reference> {
        let anchors = |expr: &DateExpr| match expr {
            DateExpr::Anchor { node, .. } => {
                node == id || node.split_once('.').is_some_and(|(head, _)| head == id)
            }
            _ => false,
        };
        let mut out = Vec::new();

        for e in self.entities.values() {
            if e.id == id {
                continue;
            }
            if e.parents.iter().any(|p| p == id) {
                out.push(Reference { by: e.id.clone(), how: "parent" });
            }
            if let Some(span) = &e.existence
                && (anchors(&span.from) || anchors(&span.to))
            {
                out.push(Reference { by: e.id.clone(), how: "existence anchor" });
            }
            for f in &e.facts {
                if f.value.as_ref_id() == Some(id) {
                    out.push(Reference { by: e.id.clone(), how: "fact value" });
                } else if anchors(&f.from) || anchors(&f.to) {
                    out.push(Reference { by: e.id.clone(), how: "fact anchor" });
                }
            }
        }

        for ev in self.events.values() {
            if ev.id == id {
                continue;
            }
            if ev.participants.iter().any(|p| p == id) {
                out.push(Reference { by: ev.id.clone(), how: "participant" });
            }
            if ev.location.as_deref() == Some(id) {
                out.push(Reference { by: ev.id.clone(), how: "location" });
            }
            if anchors(&ev.date) {
                out.push(Reference { by: ev.id.clone(), how: "date anchor" });
            }
        }

        for sc in self.scenes.values() {
            if sc.id == id {
                continue;
            }
            if sc.pov.as_deref() == Some(id) {
                out.push(Reference { by: sc.id.clone(), how: "pov" });
            }
            if sc.on_page.iter().any(|p| p == id) {
                out.push(Reference { by: sc.id.clone(), how: "on the page" });
            }
            if sc.location.as_deref() == Some(id) {
                out.push(Reference { by: sc.id.clone(), how: "scene location" });
            }
            if anchors(&sc.date) {
                out.push(Reference { by: sc.id.clone(), how: "date anchor" });
            }
        }

        out
    }

    /// Everything true at one instant.
    pub fn at(&self, day: Day) -> Snapshot<'_> {
        Snapshot {
            day,
            entities: self.entities.keys().filter_map(|id| self.entity_at(id, day)).collect(),
        }
    }

    /// An entity as it stood, with only the facts valid then. `None` if it did not
    /// exist — the map should not draw it at all.
    pub fn entity_at(&self, id: &str, day: Day) -> Option<EntityView<'_>> {
        let entity = self.entities.get(id)?;
        let existence = self.lifespans.get(id)?.at(day);
        if existence == Containment::No {
            return None;
        }

        let facts = entity
            .facts
            .iter()
            .zip(&self.fact_spans[id])
            .filter_map(|(f, span)| {
                let certainty = span.at(day);
                (certainty != Containment::No).then_some(FactAt {
                    attr: &f.attr,
                    value: &f.value,
                    certainty,
                    span: *span,
                })
            })
            .collect();

        Some(EntityView { entity, existence, facts })
    }

    /// One attribute at one moment, preferring a settled fact over a possible one.
    /// Two settled facts for the same attribute is a territory conflict, which the
    /// consistency engine reports; here the first simply wins.
    pub fn value_at(&self, id: &str, attr: &str, day: Day) -> Option<FactAt<'_>> {
        let view = self.entity_at(id, day)?;
        let mut best: Option<FactAt<'_>> = None;
        for f in view.facts.into_iter().filter(|f| f.attr == attr) {
            let better = match &best {
                None => true,
                Some(b) => b.certainty != Containment::Yes && f.certainty == Containment::Yes,
            };
            if better {
                best = Some(f);
            }
        }
        best
    }

    /// Events whose date window touches `[from, to]`, in chronological order.
    /// Events with no date at all are omitted — they have nowhere to sit.
    pub fn events_between(&self, from: Day, to: Day) -> Vec<&Event> {
        let window = Interval::new(Some(from), Some(to.offset(1)));
        let mut out: Vec<&Event> = self
            .events
            .values()
            .filter(|e| {
                let r = self.resolved[&e.id];
                r.nominal.is_some() && Interval::inclusive(r.earliest, r.latest).overlaps(&window)
            })
            .collect();
        out.sort_by_key(|e| self.resolved[&e.id].nominal.unwrap_or(Day::MIN));
        out
    }

    /// Ancestors, breadth-first. Lineage needs no subsystem: it is parentage edges over
    /// entities that already carry existence intervals.
    pub fn ancestors(&self, id: &str, depth: usize) -> Vec<&Entity> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut frontier = vec![id.to_string()];

        for _ in 0..depth {
            let mut next = Vec::new();
            for current in &frontier {
                let Some(entity) = self.entities.get(current) else { continue };
                for parent_id in &entity.parents {
                    if !seen.insert(parent_id.clone()) {
                        continue;
                    }
                    if let Some(parent) = self.entities.get(parent_id) {
                        out.push(parent);
                        next.push(parent_id.clone());
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        out
    }

    /// Resolve an ad-hoc date expression against this world, so `@evt_siege+2y` works
    /// from a query box as well as from a file.
    pub fn day_of(&self, expr: &str) -> Result<Option<Day>> {
        let parsed = parse_date(expr)?;
        let resolver = Resolver::new(&self.calendar).with_fuzz(self.fuzz);
        Ok(resolver.resolve_in("query", &parsed, &self.resolved)?.nominal)
    }

    pub fn resolved_node(&self, node: &str) -> Option<Resolved> {
        self.resolved.get(node).copied()
    }

    /// Resolve a date expression as if it had been written inside `owner`'s file, so
    /// `@self.death` and `@evt_siege+2y` both mean what they mean there.
    pub fn resolve_in(&self, owner: &str, expr: &wb_core::DateExpr) -> Result<Resolved> {
        let resolver = Resolver::new(&self.calendar).with_fuzz(self.fuzz);
        Ok(resolver.resolve_in(owner, expr, &self.resolved)?)
    }

    pub fn lifespan(&self, id: &str) -> Option<&FuzzyInterval> {
        self.lifespans.get(id)
    }

    /// Every fact on an entity, paired with the window it holds over. Empty for an
    /// unknown id, so callers need not special-case a missing entity.
    pub fn facts_of(&self, id: &str) -> impl Iterator<Item = (&Fact, &FuzzyInterval)> {
        let facts = self.entities.get(id).map_or(&[][..], |e| e.facts.as_slice());
        let spans = self.fact_spans.get(id).map_or(&[][..], |v| v.as_slice());
        facts.iter().zip(spans)
    }

    /// The span of days an event could have fallen on, as a half-open interval.
    pub fn event_extent(&self, id: &str) -> Option<Interval> {
        let r = self.resolved_node(id)?;
        Some(Interval::inclusive(r.earliest, r.latest))
    }

    /// Ids that exist, whether entity or event — the two share one namespace.
    pub fn knows(&self, id: &str) -> bool {
        self.entities.contains_key(id) || self.events.contains_key(id)
    }

    pub fn primitive_of(&self, entity: &Entity) -> Option<Primitive> {
        self.types.get(&entity.type_name).map(|t| t.primitive)
    }

    /// Sorted instants where anything could change. Scrubbing between two adjacent
    /// points cannot alter the snapshot, so no requery is needed until one is crossed.
    pub fn change_points(&self) -> &[Day] {
        &self.change_points
    }

    /// Type names used but never declared, with the entities using them. Almost always
    /// typos; surfaced so the UI can offer "did you mean…" rather than failing to load.
    pub fn undeclared_types(&self) -> &BTreeMap<String, Vec<String>> {
        &self.undeclared_types
    }

    /// Free-text search over ids, names, types, fact values, and prose.
    ///
    /// Ranked rather than filtered, because the useful answer to "marrow" is the city
    /// first and the twelve paragraphs mentioning it after. Substring matching, not
    /// stemming — invented names are exactly the words a stemmer gets wrong.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit<'_>> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }

        let mut hits: Vec<Hit<'_>> = Vec::new();

        for e in self.entities.values() {
            let mut best: Option<(u32, &str, String)> = None;
            let mut consider = |score: u32, field: &'static str, excerpt: String| {
                if best.as_ref().is_none_or(|(s, _, _)| score > *s) {
                    best = Some((score, field, excerpt));
                }
            };

            if let Some(score) = rank(&e.id, &needle) {
                consider(score + 20, "id", e.id.clone());
            }
            if let Some(score) = rank(&e.name, &needle) {
                consider(score + 30, "name", e.name.clone());
            }
            if let Some(score) = rank(&e.type_name, &needle) {
                consider(score, "type", e.type_name.clone());
            }
            for f in &e.facts {
                let text = f.value.to_string();
                if let Some(score) = rank(&text, &needle) {
                    consider(score + 10, "fact", format!("{} = {text}", f.attr));
                }
            }
            if let Some(excerpt) = excerpt(&e.body, &needle) {
                consider(5, "prose", excerpt);
            }

            if let Some((score, field, excerpt)) = best {
                hits.push(Hit {
                    id: &e.id,
                    name: &e.name,
                    kind: &e.type_name,
                    is_event: false,
                    matched: field,
                    excerpt,
                    score,
                });
            }
        }

        for ev in self.events.values() {
            let matched = rank(&ev.name, &needle)
                .map(|s| (s + 30, "name", ev.name.clone()))
                .or_else(|| rank(&ev.id, &needle).map(|s| (s + 20, "id", ev.id.clone())))
                .or_else(|| excerpt(&ev.body, &needle).map(|x| (5, "prose", x)));

            if let Some((score, field, excerpt)) = matched {
                hits.push(Hit {
                    id: &ev.id,
                    name: &ev.name,
                    kind: if ev.kind.is_empty() { "event" } else { &ev.kind },
                    is_event: true,
                    matched: field,
                    excerpt,
                    score,
                });
            }
        }

        hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.id.cmp(b.id)));
        hits.truncate(limit);
        hits
    }
}

/// One search result, and which field earned it.
#[derive(Debug, Clone)]
pub struct Hit<'a> {
    pub id: &'a str,
    pub name: &'a str,
    /// The declared type for an entity, the event kind for an event.
    pub kind: &'a str,
    pub is_event: bool,
    pub matched: &'static str,
    /// Enough surrounding text to judge the hit without opening the record.
    pub excerpt: String,
    pub score: u32,
}

/// Higher is better: whole-string, then prefix, then anywhere. `None` if absent.
fn rank(haystack: &str, needle: &str) -> Option<u32> {
    let lower = haystack.to_lowercase();
    if lower == needle {
        Some(3)
    } else if lower.starts_with(needle) {
        Some(2)
    } else if lower.contains(needle) {
        Some(1)
    } else {
        None
    }
}

/// The sentence-ish window around the first match, so a prose hit is readable.
fn excerpt(body: &str, needle: &str) -> Option<String> {
    let at = body.to_lowercase().find(needle)?;
    let start = body[..at].char_indices().rev().nth(60).map_or(0, |(i, _)| i);
    let end = body[at..].char_indices().nth(140).map_or(body.len(), |(i, _)| at + i);
    let mut out = body[start..end].replace('\n', " ").trim().to_string();
    if start > 0 {
        out.insert(0, '…');
    }
    if end < body.len() {
        out.push('…');
    }
    Some(out)
}
