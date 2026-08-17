//! Descent, and the batons that pass along it.
//!
//! DESIGN.md §3.1 makes a claim this module has to honour rather than undermine:
//! *"lineage is **not** a special subsystem. A bloodline is actors with parentage edges
//! and overlapping existence intervals. A dynasty is a polity whose ruling-title interval
//! passes along those edges. Same primitives, no bespoke genealogy engine."*
//!
//! So there is no family-tree type here, no marriage edge, no house record. There are
//! two walks over `parents:` and one grouping over facts that already exist, and the
//! whole dynasty view is drawn from those.
//!
//! ## The grouping is a transpose, and that is the new part
//!
//! `wb_check::rules::succession_gaps` runs **per entity, per attribute**: one person's
//! own title, and the holes in it. That answers "was Aldric ever not the duke". It cannot
//! answer "who was Duke of Corrath", because that question is about a value held by
//! *different* records at different times, and no rule in the engine looks that way round.
//!
//! [`successions`] does, and it turns out there are two shapes of baton, not one:
//!
//! - a **title** is one value passed between records — `title: "Duke of Corrath"`, held by
//!   Maren and then by Aldric;
//! - an **office** is one record's attribute passing between values — `owner` of the Vale
//!   of Corrath, held by the duchy and then by the empire.
//!
//! Both render identically: rows of records, each with the window it held the thing over.
//! Deliberately not restricted to actors and titles — the Vale changing hands at a siege
//! is the same shape as a duchy passing from father to son, and treating one of them as
//! special is exactly the bespoke subsystem §3.1 says not to build.

use std::collections::{BTreeMap, BTreeSet};

use wb_core::{Day, FuzzyInterval, Interval};

use crate::model::{Entity, Value};
use crate::world::World;

/// A relative, and how many steps away.
#[derive(Debug, Clone)]
pub struct Relative<'a> {
    pub entity: &'a Entity,
    /// 1 is a parent or a child, 2 a grandparent or grandchild.
    pub generation: usize,
}

/// Which way a baton is being passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One value, many holders: `title: "Duke of Corrath"`.
    Title,
    /// One record's attribute, many values: the `owner` of the Vale.
    Office,
}

impl Kind {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Office => "office",
        }
    }
}

/// One record's turn holding the thing.
#[derive(Debug, Clone)]
pub struct Tenure<'a> {
    pub holder: &'a Entity,
    pub span: FuzzyInterval,
}

/// A thing passed from one record to the next, in order.
#[derive(Debug, Clone)]
pub struct Succession<'a> {
    pub kind: Kind,
    pub attr: String,
    /// The value for a title, the subject's id for an office. What is being passed.
    pub of: String,
    /// How to name it in a list: `Duke of Corrath` · `The Vale of Corrath · owner`.
    pub label: String,
    /// In the order they held it, earliest possible start first.
    pub holders: Vec<Tenure<'a>>,
    /// Stretches nobody held it. Measured on the *possible* windows, exactly as
    /// `succession_gaps` measures them, so a hole here and a finding there are the same
    /// hole — two vague tenures meeting at an event leave uncertainty, not an interregnum.
    pub gaps: Vec<Interval>,
    /// Stretches where more than one record could have held it.
    ///
    /// Measured on the possible windows too, and therefore *unsettled* rather than
    /// contested: two tenures meeting at a date written `0768~` overlap here because
    /// nobody wrote the day down, which is the same reason the map hatches a border
    /// instead of handing it to whoever the code checked first.
    pub overlaps: Vec<Interval>,
}

impl Succession<'_> {
    pub fn holder_ids(&self) -> Vec<&str> {
        self.holders.iter().map(|t| t.holder.id.as_str()).collect()
    }
}

/// Ancestors, nearest generation first.
pub fn ancestors<'a>(world: &'a World, id: &str, depth: usize) -> Vec<Relative<'a>> {
    let mut out = Vec::new();
    let mut seen: BTreeSet<String> = [id.to_string()].into();
    let mut frontier = vec![id.to_string()];

    for generation in 1..=depth {
        let mut next = Vec::new();
        for current in &frontier {
            let Some(entity) = world.entities.get(current) else { continue };
            for parent_id in &entity.parents {
                if !seen.insert(parent_id.clone()) {
                    continue;
                }
                if let Some(parent) = world.entities.get(parent_id) {
                    out.push(Relative { entity: parent, generation });
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

/// Descendants, nearest generation first.
///
/// Children are not stored, so each generation is a scan for records naming the frontier
/// as a parent. Linear in the world per generation, which at the scale measured in §11
/// (20,000 records, single-digit milliseconds) is not worth an index that could fall out
/// of step with the files.
pub fn descendants<'a>(world: &'a World, id: &str, depth: usize) -> Vec<Relative<'a>> {
    let mut out = Vec::new();
    let mut seen: BTreeSet<String> = [id.to_string()].into();
    let mut frontier: BTreeSet<String> = [id.to_string()].into();

    for generation in 1..=depth {
        let mut next = BTreeSet::new();
        for entity in world.entities.values() {
            if seen.contains(&entity.id) || !entity.parents.iter().any(|p| frontier.contains(p)) {
                continue;
            }
            seen.insert(entity.id.clone());
            next.insert(entity.id.clone());
            out.push(Relative { entity, generation });
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    out
}

/// How many generations down from the oldest recorded forebear this record sits.
///
/// Zero means nobody's child, as far as this world knows — which is usually the top of
/// the tree and occasionally just a parent nobody has written down yet. A parentage cycle
/// stops the walk and reports zero rather than recursing: `impossible-parentage` is the
/// rule that complains about it, and a chart is not the place to raise it.
pub fn generation_of(world: &World, id: &str) -> usize {
    fn walk(world: &World, id: &str, path: &mut BTreeSet<String>) -> usize {
        if !path.insert(id.to_string()) {
            return 0;
        }
        let depth = world
            .entities
            .get(id)
            .map(|entity| {
                entity
                    .parents
                    .iter()
                    .filter(|p| world.entities.contains_key(*p))
                    .map(|p| walk(world, p, path) + 1)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        path.remove(id);
        depth
    }
    walk(world, id, &mut BTreeSet::new())
}

/// Everyone connected to this record by parentage, in either direction and to any depth.
///
/// A connected component, not a surname: two Vanes who share no recorded ancestor are two
/// houses here, and one house may well hold two surnames. The world's own edges decide.
pub fn house<'a>(world: &'a World, id: &str) -> Vec<&'a Entity> {
    let mut members: BTreeSet<String> = [id.to_string()].into();
    let mut growing = true;
    while growing {
        growing = false;
        for entity in world.entities.values() {
            let touches =
                members.contains(&entity.id) || entity.parents.iter().any(|p| members.contains(p));
            if !touches {
                continue;
            }
            if members.insert(entity.id.clone()) {
                growing = true;
            }
            for parent in &entity.parents {
                if world.entities.contains_key(parent) && members.insert(parent.clone()) {
                    growing = true;
                }
            }
        }
    }

    let mut out: Vec<&Entity> = members.iter().filter_map(|m| world.entities.get(m)).collect();
    out.sort_by_key(|e| {
        (generation_of(world, &e.id), world.lifespan(&e.id).and_then(|l| l.possible.from))
    });
    out
}

/// Where a tenure list has holes and where it doubles up.
///
/// Both are measured on the possible windows, which is the same basis the
/// `succession-gap` rule uses. Anything narrower would call every vague handover an
/// interregnum, and this world is mostly vague handovers.
fn holes_and_doubles(holders: &[Tenure<'_>]) -> (Vec<Interval>, Vec<Interval>) {
    let mut ranges: Vec<(i64, i64)> = holders
        .iter()
        .map(|t| {
            (
                t.span.possible.from.map_or(i64::MIN, |d| d.0),
                t.span.possible.to.map_or(i64::MAX, |d| d.0),
            )
        })
        .filter(|(from, to)| from < to)
        .collect();
    ranges.sort_unstable();

    let (mut gaps, mut overlaps) = (Vec::new(), Vec::new());
    let Some(&(_, first_end)) = ranges.first() else { return (gaps, overlaps) };
    let mut covered_to = first_end;

    for &(from, to) in &ranges[1..] {
        if from > covered_to {
            gaps.push(Interval::new(Some(Day(covered_to)), Some(Day(from))));
        } else if from < covered_to {
            overlaps.push(Interval::new(Some(Day(from)), Some(Day(covered_to.min(to)))));
        }
        covered_to = covered_to.max(to);
    }
    (gaps, overlaps)
}

fn name_of<'a>(world: &'a World, id: &'a str) -> &'a str {
    world.entities.get(id).map_or(id, |e| e.name.as_str())
}

/// Every thing in this world that changed hands.
///
/// Sorted with the longest chains first, because a baton passed four times is a dynasty
/// and one passed twice may just be a record with two facts on it.
pub fn successions(world: &World) -> Vec<Succession<'_>> {
    let mut titles: BTreeMap<(String, String), Vec<Tenure<'_>>> = BTreeMap::new();
    let mut offices: BTreeMap<(String, String), Vec<Tenure<'_>>> = BTreeMap::new();

    for entity in world.entities.values() {
        for (fact, span) in world.facts_of(&entity.id) {
            if world.rules.is_multi_valued(&fact.attr) {
                continue;
            }
            let Value::Text(text) = &fact.value else { continue };

            match world.entities.get(text) {
                // The value names a record, so the *value* is the holder and this
                // record's attribute is what passes: the Vale's owner, Aldric's seat.
                Some(holder) => offices
                    .entry((entity.id.clone(), fact.attr.clone()))
                    .or_default()
                    .push(Tenure { holder, span: *span }),
                // The value is a plain string, so it is the thing being held and this
                // record is one of its holders: Duke of Corrath, held by Maren.
                None => titles
                    .entry((fact.attr.clone(), text.clone()))
                    .or_default()
                    .push(Tenure { holder: entity, span: *span }),
            }
        }
    }

    let mut out = Vec::new();

    for ((attr, value), mut holders) in titles {
        let distinct: BTreeSet<&str> = holders.iter().map(|t| t.holder.id.as_str()).collect();
        if distinct.len() < 2 {
            continue;
        }
        holders.sort_by_key(|t| t.span.possible.from.unwrap_or(Day::MIN));
        let (gaps, overlaps) = holes_and_doubles(&holders);
        out.push(Succession {
            kind: Kind::Title,
            label: value.clone(),
            of: value,
            attr,
            holders,
            gaps,
            overlaps,
        });
    }

    for ((subject, attr), mut holders) in offices {
        let distinct: BTreeSet<&str> = holders.iter().map(|t| t.holder.id.as_str()).collect();
        if distinct.len() < 2 {
            continue;
        }
        holders.sort_by_key(|t| t.span.possible.from.unwrap_or(Day::MIN));
        let (gaps, overlaps) = holes_and_doubles(&holders);
        out.push(Succession {
            kind: Kind::Office,
            label: format!("{} · {attr}", name_of(world, &subject)),
            of: subject,
            attr,
            holders,
            gaps,
            overlaps,
        });
    }

    out.sort_by(|a, b| {
        b.holders
            .len()
            .cmp(&a.holders.len())
            .then_with(|| a.kind.slug().cmp(b.kind.slug()))
            .then_with(|| a.label.cmp(&b.label))
    });
    out
}
