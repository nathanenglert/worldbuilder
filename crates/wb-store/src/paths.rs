//! Where a record's file goes when nothing on disk says otherwise.
//!
//! This is format knowledge, not proposal knowledge, which is why it lives beside the
//! loader that reads these paths back. Both the review queue and the app's own authoring
//! path need to answer the same question the same way, and a world where the two
//! disagreed would scatter a writer's records across two folder conventions.
//!
//! Every function here honours an existing `source` first. A record that already has a
//! file keeps it — renaming an entity must not move it, because the writer may well have
//! put it where they wanted it.

use std::path::PathBuf;

use crate::model::{Entity, Event, Primitive};
use crate::world::World;

/// A filename-safe form of a display name: lowercase, ASCII alphanumerics, single dashes.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-').to_string();
    if trimmed.is_empty() { "untitled".to_string() } else { trimmed }
}

/// The `entities/` subfolder a record belongs in, by what kind of thing it is.
///
/// The folder is a convention for the writer's benefit — the loader reads `type:`, never
/// the directory name — so an entity filed under `misc` is as loadable as any other.
pub fn folder_for(world: &World, entity: &Entity) -> &'static str {
    match world.primitive_of(entity) {
        Some(Primitive::Actor) => "actors",
        Some(Primitive::Polity) => "polities",
        Some(Primitive::Place) => "places",
        Some(Primitive::Thing) => "things",
        Some(Primitive::Event) | None => "misc",
    }
}

pub fn entity_path(world: &World, entity: &Entity) -> PathBuf {
    if !entity.source.as_os_str().is_empty() {
        return entity.source.clone();
    }
    world
        .root
        .join("entities")
        .join(folder_for(world, entity))
        .join(format!("{}.md", slug(&entity.name)))
}

pub fn event_path(world: &World, event: &Event) -> PathBuf {
    if !event.source.as_os_str().is_empty() {
        return event.source.clone();
    }
    // Events sort by date in the folder listing, the way the existing ones do.
    let year = world
        .resolved_node(&event.id)
        .and_then(|r| r.nominal)
        .map(|d| world.calendar.from_day(d).year)
        .unwrap_or(0);
    world.root.join("events").join(format!("{:04}-{}.yaml", year, slug(&event.name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_becomes_a_filename() {
        assert_eq!(slug("The Vale of Corrath"), "the-vale-of-corrath");
        assert_eq!(slug("Hold Vashen"), "hold-vashen");
        assert_eq!(slug("  spaced  out  "), "spaced-out");
        assert_eq!(slug("Æthel's Rest"), "thel-s-rest");
    }

    /// A name with nothing a filename can use still has to land somewhere.
    #[test]
    fn a_name_with_no_usable_characters_is_still_given_a_file() {
        assert_eq!(slug("———"), "untitled");
        assert_eq!(slug(""), "untitled");
    }
}
