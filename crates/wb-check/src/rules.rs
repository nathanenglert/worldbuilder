//! The six deterministic rules.
//!
//! None of this needs a model. Every check below is interval arithmetic over facts the
//! writer already stated, which makes it instant, offline, and incapable of inventing
//! a contradiction that is not there.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use wb_core::{Day, FuzzyInterval, Interval};
use wb_store::{Entity, Fact, World};

use crate::finding::{Certainty, Finding, Rule};

// ---------------------------------------------------------------- helpers

fn label(world: &World, id: &str) -> String {
    world
        .entities
        .get(id)
        .map(|e| e.name.clone())
        .or_else(|| world.events.get(id).map(|e| e.name.clone()))
        .or_else(|| world.scenes.get(id).map(|s| s.name.clone()))
        .unwrap_or_else(|| id.to_string())
}

fn source_of(world: &World, id: &str) -> Option<PathBuf> {
    world
        .entities
        .get(id)
        .map(|e| e.source.clone())
        .or_else(|| world.events.get(id).map(|e| e.source.clone()))
        .or_else(|| world.scenes.get(id).map(|s| s.source.clone()))
}

fn sources(world: &World, ids: &[&str]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = ids.iter().filter_map(|id| source_of(world, id)).collect();
    out.dedup();
    out
}

fn when(world: &World, iv: &Interval) -> String {
    let day = |d: Day| world.calendar.format_long(d);
    match (iv.from, iv.to) {
        // `to` is exclusive; a reader wants the last day it covered.
        (Some(a), Some(b)) if b.0 - a.0 <= 1 => day(a),
        (Some(a), Some(b)) => format!("{} to {}", day(a), day(b.offset(-1))),
        (Some(a), None) => format!("{} onwards", day(a)),
        (None, Some(b)) => format!("up to {}", day(b.offset(-1))),
        (None, None) => "all of time".to_string(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Placement {
    Inside,
    Doubtful,
    Outside,
}

/// Where a claim sits relative to something's existence.
///
/// `Outside` means no reading of either set of dates lets them coexist. `Doubtful`
/// means the certain lifetime does not cover the whole claim, but the vague edges
/// might — which is exactly what a deliberately unresolved death date produces.
fn placement(claim: &FuzzyInterval, life: &FuzzyInterval) -> Placement {
    if claim.possible.is_empty() {
        return Placement::Inside;
    }
    if !life.possible.overlaps(&claim.possible) {
        return Placement::Outside;
    }
    if life.certain.covers(&claim.certain) {
        return Placement::Inside;
    }
    Placement::Doubtful
}

fn sharp(iv: Interval) -> FuzzyInterval {
    FuzzyInterval { certain: iv, possible: iv }
}

/// True when this event is what bounds the entity's existence.
///
/// A duchy annexed at the siege participates in the siege; a city founded by a founding
/// hosts it. Their lifespans end or begin exactly there, so the boundary always looks
/// like a near-miss — and reporting it would flag the most ordinary shape in the world.
fn bounded_by(entity: &Entity, event_id: &str) -> bool {
    entity.existence.as_ref().is_some_and(|span| {
        span.from.depends_on() == Some(event_id) || span.to.depends_on() == Some(event_id)
    })
}

// ---------------------------------------------------------------- rules

/// An event or scene naming someone or somewhere that was not around for it.
pub(crate) fn existence_violations(world: &World, out: &mut Vec<Finding>) {
    for event in world.events.values() {
        let roles: Vec<(&str, &String)> = event
            .participants
            .iter()
            .map(|id| ("took part in", id))
            .chain(event.location.iter().map(|id| ("hosted", id)))
            .collect();
        dated_roles(world, &event.id, &event.name, &roles, out);
    }

    // A scene is a dated record naming records, which is the same shape and the same
    // check. The verbs differ because the claim is weaker: a scene says somebody was on
    // the page, not that they did anything.
    for scene in world.scenes.values() {
        let roles: Vec<(&str, &String)> = scene
            .pov
            .iter()
            .map(|id| ("is the point of view of", id))
            .chain(scene.on_page.iter().map(|id| ("appears in", id)))
            .chain(scene.location.iter().map(|id| ("is set in", id)))
            .collect();
        dated_roles(world, &scene.id, &scene.name, &roles, out);
    }
}

/// The body of [`existence_violations`], for one dated record and the ids it names.
fn dated_roles(
    world: &World,
    id: &str,
    name: &str,
    roles: &[(&str, &String)],
    out: &mut Vec<Finding>,
) {
    let Some(resolved) = world.resolved_node(id) else { return };
    // A record with no position on the timeline cannot contradict anyone's dates.
    if resolved.nominal.is_none() || (resolved.earliest.is_none() && resolved.latest.is_none()) {
        return;
    }
    let extent = Interval::inclusive(resolved.earliest, resolved.latest);
    let window = sharp(extent);

    for (verb, target) in roles {
        let Some(life) = world.lifespan(target) else { continue };
        if world.entities.get(*target).is_some_and(|e| bounded_by(e, id)) {
            continue;
        }
        let certainty = match placement(&window, life) {
            Placement::Inside => continue,
            Placement::Outside => Certainty::Definite,
            Placement::Doubtful => Certainty::Possible,
        };

        let who = label(world, target);
        let message = match certainty {
            Certainty::Definite => format!(
                "{who} {verb} “{name}” ({}), but existed only {}.",
                when(world, &extent),
                when(world, &life.possible)
            ),
            Certainty::Possible => format!(
                "{who} {verb} “{name}” ({}), which may fall outside their existence — \
                 certainly {}, possibly {}.",
                when(world, &extent),
                when(world, &life.certain),
                when(world, &life.possible)
            ),
        };

        out.push(Finding {
            rule: Rule::ExistenceViolation,
            certainty,
            subject: id.to_string(),
            related: vec![(*target).clone()],
            message,
            at: resolved.nominal,
            sources: sources(world, &[id, target]),
        });
    }
}

/// A fact pointing at something that did not exist while the fact held.
pub(crate) fn anachronistic_facts(world: &World, out: &mut Vec<Finding>) {
    for entity in world.entities.values() {
        for (fact, span) in world.facts_of(&entity.id) {
            let Some(target) = fact.value.as_ref_id() else { continue };
            if target == entity.id {
                continue;
            }
            let Some(life) = world.lifespan(target) else { continue };

            let certainty = match placement(span, life) {
                Placement::Inside => continue,
                Placement::Outside => Certainty::Definite,
                Placement::Doubtful => Certainty::Possible,
            };

            let target_name = label(world, target);
            let message = match certainty {
                Certainty::Definite => format!(
                    "{}'s {} is {} for {}, but {} existed only {}.",
                    entity.name,
                    fact.attr,
                    target_name,
                    when(world, &span.possible),
                    target_name,
                    when(world, &life.possible)
                ),
                Certainty::Possible => format!(
                    "{}'s {} is {} for {}, which may reach beyond {}'s existence ({}).",
                    entity.name,
                    fact.attr,
                    target_name,
                    when(world, &span.possible),
                    target_name,
                    when(world, &life.certain)
                ),
            };

            out.push(Finding {
                rule: Rule::AnachronisticFact,
                certainty,
                subject: entity.id.clone(),
                related: vec![target.to_string()],
                message,
                at: span.possible.from,
                sources: sources(world, &[&entity.id, target]),
            });
        }
    }
}

/// One attribute asserted two ways over days where both are settled.
///
/// Only *certain* overlaps are reported. Two claims overlapping merely in their vague
/// edges is the contested-border case — the thing the map is built to show — and
/// flagging it would turn the feature into an error.
pub(crate) fn conflicting_facts(world: &World, out: &mut Vec<Finding>) {
    for entity in world.entities.values() {
        let mut by_attr: BTreeMap<&str, Vec<(&Fact, &FuzzyInterval)>> = BTreeMap::new();
        for (fact, span) in world.facts_of(&entity.id) {
            if world.rules.is_multi_valued(&fact.attr) {
                continue;
            }
            by_attr.entry(fact.attr.as_str()).or_default().push((fact, span));
        }

        for (attr, group) in by_attr {
            for i in 0..group.len() {
                for j in (i + 1)..group.len() {
                    let (a, a_span) = group[i];
                    let (b, b_span) = group[j];
                    if a.value == b.value {
                        continue;
                    }
                    let Some(overlap) = a_span.certain.intersect(&b_span.certain) else { continue };

                    out.push(Finding {
                        rule: Rule::ConflictingFacts,
                        certainty: Certainty::Definite,
                        subject: entity.id.clone(),
                        related: Vec::new(),
                        message: format!(
                            "{}'s {attr} is both {} and {} during {}.",
                            entity.name,
                            a.value,
                            b.value,
                            when(world, &overlap)
                        ),
                        at: overlap.from,
                        sources: sources(world, &[&entity.id]),
                    });
                }
            }
        }
    }
}

/// References to ids nothing defines.
pub(crate) fn orphan_references(world: &World, out: &mut Vec<Finding>) {
    // Self-calibrating: only values whose prefix is already an id prefix somewhere in
    // this world count as references, so `iron_ore` is a value and `pol_typo` is a typo.
    let prefixes: BTreeSet<&str> = world
        .entities
        .keys()
        .chain(world.events.keys())
        .chain(world.scenes.keys())
        .filter_map(|id| id.split_once('_').map(|(prefix, _)| prefix))
        .collect();

    let report = |subject: &str, target: &str, role: &str, out: &mut Vec<Finding>| {
        out.push(Finding {
            rule: Rule::OrphanReference,
            certainty: Certainty::Definite,
            subject: subject.to_string(),
            related: vec![target.to_string()],
            message: format!(
                "{} names `{target}` as {role}, which nothing defines.",
                label(world, subject)
            ),
            at: None,
            sources: sources(world, &[subject]),
        });
    };

    for entity in world.entities.values() {
        for parent in &entity.parents {
            if !world.knows(parent) {
                report(&entity.id, parent, "a parent", out);
            }
        }
        for (fact, _) in world.facts_of(&entity.id) {
            let Some(value) = fact.value.as_ref_id() else { continue };
            let Some((prefix, _)) = value.split_once('_') else { continue };
            if !prefixes.contains(prefix) || world.knows(value) {
                continue;
            }
            report(&entity.id, value, &format!("its {}", fact.attr), out);
        }
    }

    for event in world.events.values() {
        for participant in &event.participants {
            if !world.knows(participant) {
                report(&event.id, participant, "a participant", out);
            }
        }
        if let Some(location) = &event.location
            && !world.knows(location)
        {
            report(&event.id, location, "its location", out);
        }
    }

    // A scene names records the same three ways an event does, and a typo in a `pov:`
    // is exactly as much a dangling reference as one in `participants:`.
    for scene in world.scenes.values() {
        if let Some(pov) = &scene.pov
            && !world.knows(pov)
        {
            report(&scene.id, pov, "its point of view", out);
        }
        for id in &scene.on_page {
            if !world.knows(id) {
                report(&scene.id, id, "on the page", out);
            }
        }
        if let Some(location) = &scene.location
            && !world.knows(location)
        {
            report(&scene.id, location, "its location", out);
        }
    }
}

/// A single-valued attribute with a stretch nothing covers.
pub(crate) fn succession_gaps(world: &World, out: &mut Vec<Finding>) {
    for entity in world.entities.values() {
        let mut by_attr: BTreeMap<&str, Vec<&FuzzyInterval>> = BTreeMap::new();
        for (fact, span) in world.facts_of(&entity.id) {
            if world.rules.is_multi_valued(&fact.attr) {
                continue;
            }
            by_attr.entry(fact.attr.as_str()).or_default().push(span);
        }

        for (attr, spans) in by_attr {
            if spans.len() < 2 {
                continue;
            }

            // Measured on the *possible* windows: two facts meeting at a vague event
            // leave a hole between their certain cores, and that hole is uncertainty,
            // not an unruled decade.
            let mut ranges: Vec<(i64, i64)> = spans
                .iter()
                .map(|s| {
                    (
                        s.possible.from.map_or(i64::MIN, |d| d.0),
                        s.possible.to.map_or(i64::MAX, |d| d.0),
                    )
                })
                .filter(|(from, to)| from < to)
                .collect();
            ranges.sort_unstable();
            if ranges.len() < 2 {
                continue;
            }

            let mut covered_to = ranges[0].1;
            for &(from, to) in &ranges[1..] {
                if from > covered_to {
                    let gap = Interval::new(Some(Day(covered_to)), Some(Day(from)));
                    out.push(Finding {
                        rule: Rule::SuccessionGap,
                        certainty: Certainty::Definite,
                        subject: entity.id.clone(),
                        related: Vec::new(),
                        message: format!(
                            "{} has no {attr} for {}.",
                            entity.name,
                            when(world, &gap)
                        ),
                        at: Some(Day(covered_to)),
                        sources: sources(world, &[&entity.id]),
                    });
                }
                covered_to = covered_to.max(to);
            }
        }
    }
}

/// Children who arrive before their parents, or too long after them.
pub(crate) fn impossible_parentage(world: &World, out: &mut Vec<Finding>) {
    let gestation = world.rules.gestation_days;

    for child in world.entities.values() {
        if child.parents.is_empty() {
            continue;
        }
        let Some(birth) = world.resolved_node(&format!("{}.birth", child.id)) else { continue };

        for parent_id in &child.parents {
            let Some(parent) = world.entities.get(parent_id) else { continue };
            let parent_birth = world.resolved_node(&format!("{parent_id}.birth"));
            let parent_death = world.resolved_node(&format!("{parent_id}.death"));

            let push = |certainty, message, at, out: &mut Vec<Finding>| {
                out.push(Finding {
                    rule: Rule::ImpossibleParentage,
                    certainty,
                    subject: child.id.clone(),
                    related: vec![parent_id.clone()],
                    message,
                    at,
                    sources: sources(world, &[&child.id, parent_id]),
                });
            };

            // Born before the parent was.
            if let (Some(child_latest), Some(parent_earliest)) =
                (birth.latest, parent_birth.and_then(|r| r.earliest))
                && child_latest < parent_earliest
            {
                push(
                    Certainty::Definite,
                    format!(
                        "{} was born by {}, before their parent {} could have been ({}).",
                        child.name,
                        world.calendar.format_long(child_latest),
                        parent.name,
                        world.calendar.format_long(parent_earliest)
                    ),
                    Some(child_latest),
                    out,
                );
                continue;
            }
            if let (Some(child_earliest), Some(parent_latest)) =
                (birth.earliest, parent_birth.and_then(|r| r.latest))
                && child_earliest < parent_latest
            {
                push(
                    Certainty::Possible,
                    format!(
                        "{} may predate their parent {} — the two birth windows overlap.",
                        child.name, parent.name
                    ),
                    Some(child_earliest),
                    out,
                );
            }

            // Born too long after the parent died.
            if let (Some(child_earliest), Some(parent_gone)) =
                (birth.earliest, parent_death.and_then(|r| r.latest))
                && child_earliest.0 > parent_gone.0 + gestation
            {
                push(
                    Certainty::Definite,
                    format!(
                        "{} was born no earlier than {}, more than {gestation} days after their \
                         parent {} died ({}).",
                        child.name,
                        world.calendar.format_long(child_earliest),
                        parent.name,
                        world.calendar.format_long(parent_gone)
                    ),
                    Some(child_earliest),
                    out,
                );
            }
        }
    }
}
