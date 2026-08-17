//! Turning a writer's text into a document nobody has to trust.
//!
//! Two jobs, and both of them are the kind that fail quietly. Escaping is the obvious
//! one: a record called `Aldric <the Younger>` must not be able to close a tag. The
//! second is cross-linking — every `#anchor` this module emits has to land on an `id`
//! that exists in the same file, because a bible full of links that go nowhere is
//! *worse* than one with no links at all: it looks finished.
//!
//! Raw HTML in a record's prose body is passed through rather than stripped. It is the
//! writer's own file, they may well have written `<br>` in it on purpose, and this is a
//! document they are choosing to produce from their own words.

use std::collections::BTreeSet;

use pulldown_cmark::{Options, Parser, html};
use wb_store::World;

/// Prefix on every record anchor, so an id from the world can never collide with an
/// element this module put in the page for its own reasons.
pub const ANCHOR: &str = "rec-";

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// What a `[[wikilink]]` or a bare-id fact value points at, if it points at anything in
/// this document.
///
/// Resolution order is id, then exact name, then an `aka` spelling — the same three
/// things the mention scanner looks at, so a link that works in the app works here.
pub fn resolve<'a>(world: &'a World, included: &BTreeSet<String>, key: &str) -> Option<&'a str> {
    let key = key.trim();
    if included.contains(key) {
        if let Some(entity) = world.entities.get(key) {
            return Some(entity.id.as_str());
        }
        if let Some(event) = world.events.get(key) {
            return Some(event.id.as_str());
        }
    }

    let lowered = key.to_lowercase();
    world
        .entities
        .values()
        .find(|e| {
            included.contains(&e.id)
                && (e.name.to_lowercase() == lowered
                    || e.aliases.iter().any(|a| a.to_lowercase() == lowered))
        })
        .map(|e| e.id.as_str())
        .or_else(|| {
            world
                .events
                .values()
                .find(|e| included.contains(&e.id) && e.name.to_lowercase() == lowered)
                .map(|e| e.id.as_str())
        })
}

pub fn link(id: &str, text: &str) -> String {
    format!("<a href=\"#{ANCHOR}{}\">{}</a>", escape(id), escape(text))
}

/// Replace `[[target]]` and `[[target|words]]` with anchors, before the Markdown parser
/// sees them.
///
/// Done as raw HTML rather than as Markdown link syntax so the display text needs no
/// second round of escaping — link text with a `]` in it is otherwise a silent trap.
/// A link to something outside this export's scope becomes plain text: the reader is
/// not shown a door to a room that is not in the building.
fn wikilinks(world: &World, included: &BTreeSet<String>, body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;

    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else {
            out.push_str(&rest[start..]);
            return out;
        };

        let inner = &after[..end];
        let (target, text) = match inner.split_once('|') {
            Some((t, d)) => (t.trim(), d.trim()),
            None => (inner.trim(), inner.trim()),
        };
        match resolve(world, included, target) {
            Some(id) => out.push_str(&link(id, text)),
            None => out.push_str(&escape(text)),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// A record's prose body, as HTML.
pub fn prose(world: &World, included: &BTreeSet<String>, body: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        return String::new();
    }

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    // A bible is a document to be read, so quotes and dashes should look like a book's.
    options.insert(Options::ENABLE_SMART_PUNCTUATION);

    let linked = wikilinks(world, included, body);
    let mut rendered = String::new();
    html::push_html(&mut rendered, Parser::new_ext(&linked, options));
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_looks_like_markup_cannot_close_a_tag() {
        assert_eq!(
            escape("Aldric <the Younger> & Co \"IV\""),
            "Aldric &lt;the Younger&gt; &amp; Co &quot;IV&quot;"
        );
    }

    #[test]
    fn an_unclosed_wikilink_is_left_exactly_as_the_writer_typed_it() {
        let world = wb_store::load("../../examples/vashen").expect("the example world");
        let included = BTreeSet::new();
        assert_eq!(wikilinks(&world, &included, "see [[act_aldric"), "see [[act_aldric");
    }
}
