//! Loads `examples/vashen` off disk and scrubs through it.

use std::path::PathBuf;

use wb_core::{CivilDate, Containment, Day};
use wb_store::{Snapshot, Value, World, load};

fn vashen() -> World {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/vashen");
    load(&root).unwrap_or_else(|e| panic!("loading {}: {e}", root.display()))
}

fn day(world: &World, year: i64, month: u8, day: u16) -> Day {
    world.calendar.to_day(CivilDate::ymd(year, month, day)).unwrap()
}

fn owner(world: &World, at: Day) -> Option<(String, Containment)> {
    world.value_at("ter_vale_of_corrath", "owner", at).map(|f| (f.value.to_string(), f.certainty))
}

#[test]
fn loads_every_record_in_the_folder() {
    let world = vashen();
    assert_eq!(world.name, "The Vashen Reckoning");
    assert_eq!(world.entities.len(), 11);
    assert_eq!(world.events.len(), 3);

    // Map geometry rides on the entity, in normalized 0..1 coordinates.
    let vale = &world.entities["ter_vale_of_corrath"];
    assert_eq!(vale.shape.len(), 6);
    assert!(vale.shape.iter().flatten().all(|c| (0.0..=1.0).contains(c)));
    assert_eq!(world.entities["place_marrow"].marker, Some([0.43, 0.40]));
    assert_eq!(world.calendar.days_in_year(812), 360);
    assert!(
        world.undeclared_types().is_empty(),
        "example world uses types it never declares: {:?}",
        world.undeclared_types()
    );
}

#[test]
fn markdown_bodies_survive_loading() {
    let world = vashen();
    let aldric = &world.entities["act_aldric_vane"];
    assert!(aldric.body.contains("Fourth of his name"));
    assert!(aldric.source.ends_with("aldric-vane.md"));
    // Plain-YAML records simply have no prose.
    assert!(world.events["evt_siege_of_marrow"].body.is_empty());
}

#[test]
fn the_territory_changes_hands_at_the_siege() {
    let world = vashen();

    let (held, certainty) = owner(&world, day(&world, 700, 1, 1)).unwrap();
    assert_eq!(held, "pol_corrath");
    assert_eq!(certainty, Containment::Yes);

    let (held, certainty) = owner(&world, day(&world, 830, 1, 1)).unwrap();
    assert_eq!(held, "pol_vashen");
    assert_eq!(certainty, Containment::Yes);
}

#[test]
fn an_undated_day_within_the_siege_settles_nothing() {
    let world = vashen();

    // Verdant 812, written `0812-04~`: inside it, neither claim is settled.
    let (_, certainty) = owner(&world, day(&world, 812, 4, 15)).unwrap();
    assert_eq!(certainty, Containment::Maybe);

    let view = world.entity_at("ter_vale_of_corrath", day(&world, 812, 4, 15)).unwrap();
    let claims: Vec<_> = view.facts.iter().filter(|f| f.attr == "owner").collect();
    assert_eq!(claims.len(), 2, "both owners are live during the uncertainty");
    assert!(claims.iter().all(|f| f.certainty == Containment::Maybe));
}

#[test]
fn an_exactly_dated_event_flips_a_fact_cleanly() {
    let world = vashen();
    let before = day(&world, 806, 2, 13);
    let after = day(&world, 806, 2, 14); // the Oath of Vashen

    let allegiance = |at| world.value_at("act_aldric_vane", "allegiance", at).unwrap();

    assert_eq!(allegiance(before).value.to_string(), "pol_corrath");
    assert_eq!(allegiance(before).certainty, Containment::Yes);
    assert_eq!(allegiance(after).value.to_string(), "pol_vashen");
    assert_eq!(allegiance(after).certainty, Containment::Yes);
}

#[test]
fn facts_anchored_to_a_lifespan_end_when_it_does() {
    let world = vashen();

    // Aldric's title runs `to: "@act_aldric_vane.death"`, and he died "0811~".
    let title_at = |at| world.value_at("act_aldric_vane", "title", at);
    assert_eq!(title_at(day(&world, 805, 1, 1)).unwrap().certainty, Containment::Yes);
    assert_eq!(title_at(day(&world, 810, 1, 1)).unwrap().certainty, Containment::Maybe);
    assert!(title_at(day(&world, 900, 1, 1)).is_none());
}

#[test]
fn an_entity_without_dates_exists_at_every_moment() {
    let world = vashen();
    for year in [100, 812, 2000] {
        let snapshot = world.at(day(&world, year, 1, 1));
        assert!(snapshot.get("thing_high_tongue").is_some(), "the High Tongue vanished in {year}");
    }
    // Whereas a dated polity does not exist before it is founded.
    assert!(world.at(day(&world, 100, 1, 1)).get("pol_corrath").is_none());
}

#[test]
fn lineage_falls_out_of_parentage_edges() {
    let world = vashen();
    let names: Vec<&str> =
        world.ancestors("act_aldric_vane", 3).iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["Maren Vane", "Isolde Corr"]);
    assert!(world.ancestors("act_maren_vane", 3).is_empty());
}

#[test]
fn dates_can_be_asked_for_relative_to_events() {
    let world = vashen();
    let siege = world.day_of("@evt_siege_of_marrow").unwrap().unwrap();
    assert_eq!(world.calendar.from_day(siege), CivilDate::ymd(812, 4, 1));

    let after = world.day_of("@evt_siege_of_marrow+2y").unwrap().unwrap();
    assert_eq!(world.calendar.from_day(after), CivilDate::ymd(814, 4, 1));
}

#[test]
fn events_come_back_in_order() {
    let world = vashen();
    let found = world.events_between(day(&world, 0, 1, 1), day(&world, 900, 1, 1));
    let ids: Vec<&str> = found.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, vec!["evt_founding_of_corrath", "evt_oath_of_vashen", "evt_siege_of_marrow"]);

    let late = world.events_between(day(&world, 807, 1, 1), day(&world, 900, 1, 1));
    assert_eq!(late.len(), 1);
}

#[test]
fn numeric_fact_values_stay_numeric() {
    let world = vashen();
    let before = world.value_at("place_marrow", "population", day(&world, 805, 1, 1)).unwrap();
    assert_eq!(*before.value, Value::Int(9000));
}

/// The premise the scrubber's performance rests on: between two adjacent change points
/// the world is identical, so dragging across them requires no requery at all.
#[test]
fn nothing_changes_between_adjacent_change_points() {
    let world = vashen();
    let points = world.change_points();
    assert!(points.len() > 4, "expected a handful of change points, got {}", points.len());
    assert!(points.windows(2).all(|w| w[0] < w[1]), "change points must be sorted and unique");

    for pair in points.windows(2) {
        let (start, next) = (pair[0], pair[1]);
        if next.0 - start.0 < 2 {
            continue;
        }
        let midpoint = Day(start.0 + (next.0 - start.0) / 2);
        assert_eq!(
            summarize(&world.at(start)),
            summarize(&world.at(midpoint)),
            "world changed between change points {start:?} and {next:?}"
        );
    }
}

fn summarize(snapshot: &Snapshot<'_>) -> Vec<String> {
    snapshot
        .entities
        .iter()
        .map(|view| {
            let facts: Vec<String> = view
                .facts
                .iter()
                .map(|f| format!("{}={}[{:?}]", f.attr, f.value, f.certainty))
                .collect();
            format!("{} {:?} {}", view.entity.id, view.existence, facts.join(","))
        })
        .collect()
}

#[test]
fn a_folder_without_a_world_file_is_rejected() {
    let err = load(PathBuf::from(env!("CARGO_MANIFEST_DIR"))).unwrap_err();
    assert!(matches!(err, wb_store::Error::NoWorldFile { .. }), "got {err}");
}

// ------------------------------------------------------------ search

#[test]
fn search_puts_the_named_thing_above_the_things_that_mention_it() {
    let world = vashen();
    let hits = world.search("marrow", 10);

    assert_eq!(hits[0].id, "place_marrow", "the city itself ranks first: {hits:#?}");
    assert_eq!(hits[0].matched, "name");
    assert!(hits.len() > 1, "the siege and the prose mentioning it come after");
    assert!(hits.iter().any(|h| h.id == "evt_siege_of_marrow" && h.is_event));
}

#[test]
fn search_finds_prose_nothing_else_indexes_and_shows_why() {
    let world = vashen();
    let hits = world.search("eleven days", 10);

    assert_eq!(hits.len(), 1, "{hits:#?}");
    assert_eq!(hits[0].id, "place_marrow");
    assert_eq!(hits[0].matched, "prose");
    assert!(hits[0].excerpt.contains("eleven days"), "got {:?}", hits[0].excerpt);
    assert!(hits[0].excerpt.starts_with('…'), "an excerpt from mid-body is marked as one");
}

#[test]
fn search_matches_fact_values_so_a_reference_can_be_traced_back() {
    let world = vashen();
    let hits = world.search("place_vashen_seat", 10);

    assert!(
        hits.iter().any(|h| h.id == "pol_vashen" && h.matched == "fact"),
        "the empire's capital fact should surface: {hits:#?}"
    );
}

#[test]
fn search_is_bounded_and_case_insensitive_and_ignores_an_empty_query() {
    let world = vashen();
    assert_eq!(world.search("MARROW", 10)[0].id, "place_marrow");
    assert_eq!(world.search("a", 3).len(), 3, "the limit is honoured");
    assert!(world.search("   ", 10).is_empty());
    assert!(world.search("nothing here spells this", 10).is_empty());
}
