//! **wb-export** — a world as one file somebody else can open.
//!
//! A world that can only be read inside the application that made it is not a world you
//! own, whatever the file format underneath says. This crate is the way out: a whole
//! world as a single self-contained HTML document, with the map and the timeline drawn
//! inline and every record cross-linked to every other. No server, no folder of assets
//! to lose, no network at read time — email it, and it still opens in ten years.
//!
//! ## Three scopes, and one of them could not exist anywhere else
//!
//! [`Scope::AsOf`] is the reason this is not a data dump. Every fact in a world here is
//! an assertion over a window, so "the world on the twelfth of Verdant, 812" is an
//! ordinary query — and the document that falls out reads as though a chronicler wrote
//! it that year. Marrow has nine thousand people in it, because the siege has not
//! happened yet.
//!
//! [`Scope::OnThePage`] is slice 5's mention scan pointed at a different question: hand a
//! reader only the records the book has actually named them. A spoiler-free companion,
//! from the same numbers as the iceberg panel.
//!
//! ## What stays out, and why
//!
//! - **Consistency findings.** A bible is not a bug list. The open questions in a world
//!   are the writer's working notes, and shipping them to a reader turns a mystery into
//!   an erratum.
//! - **Scenes.** A scene points into a manuscript the recipient either has or should not
//!   have. This document is about the world, not about the book that reveals it.
//! - **The review queue**, for the same reason as the findings.
//! - **Terrain layers.** Derived, and reproducible from the app; the backdrop is the
//!   image the writer actually drew.

pub mod html;
mod map;
mod records;
mod style;
mod timeline;

use std::collections::BTreeSet;

use wb_core::Day;
use wb_store::{Primitive, World};

use crate::html::escape;

/// How much of the world to put in the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Every record, every fact, every window.
    Everything,
    /// Only what existed on that day, with the facts that held then and no ends that
    /// have not arrived. A gazetteer, in the voice of the year.
    AsOf(Day),
    /// Only the records the manuscript names. Spoiler-free by exposure rather than by
    /// date, and measured by the same scan the iceberg panel reports.
    OnThePage,
}

impl Scope {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Everything => "everything",
            Self::AsOf(_) => "as-of",
            Self::OnThePage => "on-the-page",
        }
    }

    fn caption(self, world: &World) -> String {
        match self {
            Self::Everything => "the whole world".to_string(),
            Self::AsOf(day) => format!("as it stood on {}", world.calendar.format_long(day)),
            Self::OnThePage => "only what the book names".to_string(),
        }
    }
}

pub(crate) fn primitive_name(p: Primitive) -> &'static str {
    match p {
        Primitive::Actor => "actor",
        Primitive::Polity => "polity",
        Primitive::Place => "place",
        Primitive::Event => "event",
        Primitive::Thing => "thing",
    }
}

/// What a scope leaves in, worked out once and then consulted by everything that draws.
///
/// One set for the whole document is what keeps every link honest: a cross-reference is
/// only rendered as a link when its target is in here, so no `href` in the finished file
/// can point at an anchor the file does not contain.
fn included(world: &World, scope: Scope) -> BTreeSet<String> {
    let mut ids: BTreeSet<String> = match scope {
        Scope::Everything => world.entities.keys().cloned().collect(),
        Scope::AsOf(day) => {
            world.at(day).entities.iter().map(|view| view.entity.id.clone()).collect()
        }
        Scope::OnThePage => {
            let story = wb_story::Story::read(world);
            wb_story::iceberg::report(world, &story)
                .entries
                .iter()
                .filter(|e| e.surfaced())
                .map(|e| e.id.clone())
                .collect()
        }
    };

    for event in world.events.values() {
        let keep = match scope {
            Scope::Everything => true,
            // An undated event is "sometime", and a chronicler writing in 812 has heard
            // of those too.
            Scope::AsOf(day) => {
                world.resolved_node(&event.id).and_then(|r| r.nominal).is_none_or(|d| d <= day)
            }
            // The reader has met somebody who was there, or the place it happened.
            Scope::OnThePage => {
                event.participants.iter().chain(event.location.as_ref()).any(|id| ids.contains(id))
            }
        };
        if keep {
            ids.insert(event.id.clone());
        }
    }

    ids
}

/// The whole world as one HTML document.
pub fn bible(world: &World, scope: Scope) -> String {
    let ids = included(world, scope);
    let day = match scope {
        Scope::AsOf(day) => day,
        _ => records::last_change(world),
    };

    let mut entities: Vec<&wb_store::Entity> =
        world.entities.values().filter(|e| ids.contains(&e.id)).collect();
    entities.sort_by(|a, b| {
        let rank = |e: &wb_store::Entity| match world.primitive_of(e) {
            Some(Primitive::Polity) => 0,
            Some(Primitive::Place) => 1,
            Some(Primitive::Actor) => 2,
            _ => 3,
        };
        rank(a).cmp(&rank(b)).then_with(|| a.name.cmp(&b.name))
    });

    let mut events: Vec<&wb_store::Event> =
        world.events.values().filter(|e| ids.contains(&e.id)).collect();
    events.sort_by_key(|e| records::event_day(world, &e.id));

    let order: Vec<String> =
        entities.iter().map(|e| e.id.clone()).chain(events.iter().map(|e| e.id.clone())).collect();

    let mut body = String::with_capacity(64 * 1024);
    body.push_str(&format!(
        "<header class=\"world wide\"><h1>{}</h1>\
         <p class=\"scope\">{} · {} · reckoned in the {}</p></header>",
        escape(&world.name),
        escape(&scope.caption(world)),
        match order.len() {
            1 => "1 record".to_string(),
            n => format!("{n} records"),
        },
        escape(&world.calendar.name)
    ));

    if let Some(figure) = map::figure(world, &ids, day) {
        body.push_str(&figure);
    }
    if let Some(figure) = timeline::figure(world, &ids) {
        body.push_str(&figure);
    }

    body.push_str("<main>");
    body.push_str("<h2 class=\"section\">Contents</h2>");
    body.push_str(&records::contents(world, &ids, &order));

    let mut last = "";
    for entity in &entities {
        let kind = world.primitive_of(entity).map(primitive_name).unwrap_or("record");
        if kind != last {
            body.push_str(&format!("<h2 class=\"section\">{}</h2>", escape(&heading(kind))));
            last = kind;
        }
        body.push_str(&records::entity(world, &ids, &scope, entity));
    }

    if !events.is_empty() {
        body.push_str("<h2 class=\"section\">Events</h2>");
        for event in &events {
            body.push_str(&records::event(world, &ids, &scope, event));
        }
    }
    body.push_str("</main>");

    body.push_str(
        "<footer>Exported from Worldbuilder. Nothing in this file is fetched from \
         anywhere — the map, the timeline and the type are all in it.<br>\
         A date written “about 0811” is one the world does not know exactly, and one \
         written “0500” is known only to the year. The vagueness is deliberate, and \
         is not a gap somebody forgot to fill.</footer>",
    );

    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <title>{}</title><style>{}</style></head><body>{body}</body></html>\n",
        escape(&world.name),
        style::CSS
    )
}

fn heading(kind: &str) -> String {
    match kind {
        "polity" => "Powers".to_string(),
        "place" => "Places".to_string(),
        "actor" => "People".to_string(),
        "thing" => "Things".to_string(),
        other => other.to_string(),
    }
}
