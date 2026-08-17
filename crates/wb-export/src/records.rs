//! The records themselves.
//!
//! Dates go through [`wb_store::phrasing::phrase`], which keeps the precision the writer
//! gave them and translates only the anchors. Resolving them all to day numbers here
//! would print the founding — dated `0500` — as `0500-01-01`, inventing two fields the
//! world never claimed, in a document whose whole subject is how much a world is allowed
//! not to know.

use std::collections::BTreeSet;

use wb_core::{DateExpr, Day};
use wb_store::{Entity, Event, Value, World};

use wb_store::phrasing::phrase;

use crate::html::{ANCHOR, escape, link, prose, resolve};
use crate::{Scope, primitive_name};

/// `0799 → about 0811`, `from 0410`, `until 0812-04`, or nothing at all.
///
/// Under [`Scope::AsOf`] the closing end is dropped: a gazetteer written in 812 does not
/// know when the duke will die, and printing the end of a window that has not happened is
/// the one way this scope could lie.
fn window(world: &World, owner: &str, from: &DateExpr, to: &DateExpr, scope: &Scope) -> String {
    let start = phrase(world, owner, from);
    let end = if matches!(scope, Scope::AsOf(_)) { None } else { phrase(world, owner, to) };

    match (start, end) {
        (None, None) => String::new(),
        (Some(s), None) => format!("from {s}"),
        (None, Some(e)) => format!("until {e}"),
        (Some(s), Some(e)) => format!("{s} → {e}"),
    }
}

fn value_html(world: &World, included: &BTreeSet<String>, value: &Value) -> String {
    match value.as_ref_id().and_then(|id| resolve(world, included, id)) {
        Some(id) => {
            let name = world.entities.get(id).map_or(id, |e| e.name.as_str());
            link(id, name)
        }
        None => escape(&value.to_string()),
    }
}

pub fn entity(
    world: &World,
    included: &BTreeSet<String>,
    scope: &Scope,
    entity: &Entity,
) -> String {
    let mut out =
        format!("<article id=\"{ANCHOR}{}\"><h3>{}</h3>", escape(&entity.id), escape(&entity.name));

    let kind = world.primitive_of(entity).map(primitive_name).unwrap_or("record");
    let mut meta = vec![escape(&entity.type_name), kind.to_string()];
    if let Some(span) = &entity.existence {
        let when = window(world, &entity.id, &span.from, &span.to, scope);
        if !when.is_empty() {
            meta.push(escape(&when));
        }
    }
    out.push_str(&format!("<p class=\"meta\">{}</p>", meta.join(" · ")));

    if !entity.aliases.is_empty() {
        out.push_str(&format!(
            "<p class=\"aka\">also called {}</p>",
            entity.aliases.iter().map(|a| escape(a)).collect::<Vec<_>>().join(", ")
        ));
    }

    // Under `AsOf` the facts have already been filtered to the ones live that day, so an
    // empty table there means "nothing was true of this then", which is worth the silence.
    let facts: Vec<&wb_store::Fact> = match scope {
        Scope::AsOf(day) => world
            .entity_at(&entity.id, *day)
            .map(|view| {
                let live: BTreeSet<(&str, String)> =
                    view.facts.iter().map(|f| (f.attr, f.value.to_string())).collect();
                entity
                    .facts
                    .iter()
                    .filter(|f| live.contains(&(f.attr.as_str(), f.value.to_string())))
                    .collect()
            })
            .unwrap_or_default(),
        _ => entity.facts.iter().collect(),
    };

    if !facts.is_empty() {
        out.push_str(
            "<div class=\"scroll\"><table class=\"facts\">\
             <tr><th>what</th><th>value</th><th>when</th></tr>",
        );
        for fact in facts {
            // A fact with no dates on it holds for every moment the record exists, which
            // is a statement and not a missing value — so the cell says so rather than
            // sitting empty and reading as an omission.
            let when = window(world, &entity.id, &fact.from, &fact.to, scope);
            out.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"when\">{}</td></tr>",
                escape(&fact.attr),
                value_html(world, included, &fact.value),
                escape(if when.is_empty() { "always" } else { &when })
            ));
        }
        out.push_str("</table></div>");
    }

    let kin: Vec<String> = entity
        .parents
        .iter()
        .filter_map(|p| resolve(world, included, p))
        .map(|id| link(id, world.entities.get(id).map_or(id, |e| e.name.as_str())))
        .collect();
    if !kin.is_empty() {
        out.push_str(&format!("<p class=\"meta\">child of {}</p>", kin.join(", ")));
    }

    out.push_str(&prose(world, included, &entity.body));
    out.push_str("</article>");
    out
}

pub fn event(world: &World, included: &BTreeSet<String>, scope: &Scope, event: &Event) -> String {
    let mut out =
        format!("<article id=\"{ANCHOR}{}\"><h3>{}</h3>", escape(&event.id), escape(&event.name));

    let mut meta = Vec::new();
    if !event.kind.is_empty() {
        meta.push(escape(&event.kind));
    }
    if let Some(when) = phrase(world, &event.id, &event.date) {
        meta.push(escape(&when));
    }
    if let Some(id) = event.location.as_deref().and_then(|l| resolve(world, included, l)) {
        meta.push(format!(
            "at {}",
            link(id, world.entities.get(id).map_or(id, |e| e.name.as_str()))
        ));
    }
    if !meta.is_empty() {
        out.push_str(&format!("<p class=\"meta\">{}</p>", meta.join(" · ")));
    }

    let cast: Vec<String> = event
        .participants
        .iter()
        .filter_map(|p| resolve(world, included, p))
        .map(|id| link(id, world.entities.get(id).map_or(id, |e| e.name.as_str())))
        .collect();
    if !cast.is_empty() {
        out.push_str(&format!("<p class=\"meta\">involving {}</p>", cast.join(" · ")));
    }

    let _ = scope;
    out.push_str(&prose(world, included, &event.body));
    out.push_str("</article>");
    out
}

/// The date an event sorts by, for the contents and the record order.
pub fn event_day(world: &World, id: &str) -> i64 {
    world.resolved_node(id).and_then(|r| r.nominal).map_or(i64::MAX, |d| d.0)
}

pub fn contents(world: &World, included: &BTreeSet<String>, ids: &[String]) -> String {
    let mut out = String::from("<ul class=\"contents\">");
    for id in ids {
        let (name, kind) = match (world.entities.get(id), world.events.get(id)) {
            (Some(entity), _) => (
                entity.name.as_str(),
                world.primitive_of(entity).map(primitive_name).unwrap_or("record"),
            ),
            (_, Some(event)) => (event.name.as_str(), "event"),
            _ => continue,
        };
        out.push_str(&format!(
            "<li>{} <span class=\"kind\">{}</span></li>",
            link(id, name),
            escape(kind)
        ));
    }
    let _ = included;
    out.push_str("</ul>");
    out
}

/// A day to draw the map at, when the scope did not name one: the last instant anything
/// in this world changes, which is the world as it ends up.
pub fn last_change(world: &World) -> Day {
    world.change_points().last().copied().unwrap_or(Day(0))
}
