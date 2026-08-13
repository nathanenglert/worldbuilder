//! An assembled world, and the time-indexed queries the map and timeline run against.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use wb_core::{
    Calendar, Containment, Day, Fuzz, FuzzyInterval, Interval, NodeMap, Resolved, Resolver,
    change_points, parse_date,
};

use crate::error::{Error, Result};
use crate::model::{Entity, Event, Fact, Primitive, Rules, TypeDef, Value, WorldDef};

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
    pub types: BTreeMap<String, TypeDef>,
    pub entities: BTreeMap<String, Entity>,
    pub events: BTreeMap<String, Event>,

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

        let change_points = change_points(spans_seen);

        Ok(Self {
            root,
            name: def.name,
            calendar: def.calendar,
            fuzz: def.fuzz,
            rules: def.rules,
            types,
            entities: entity_map,
            events: event_map,
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
            types: self.types.values().cloned().collect(),
            rules: self.rules.clone(),
        }
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
