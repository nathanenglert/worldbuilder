//! What the UI actually receives.
//!
//! The map's whole job is to render these payloads, so the payloads are where the
//! rendering decisions get checked — which polity holds a region, what colour that is,
//! and whether the claim is settled enough to draw a solid border.

use std::collections::BTreeSet;
use std::path::PathBuf;

use wb_core::{CivilDate, Day};
use wb_store::{World, load};
use worldbuilder_lib::commands::{SnapshotDto, WorldSummary};
use worldbuilder_lib::edit::references_of;

fn vashen() -> World {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/vashen");
    load(&root).unwrap_or_else(|e| panic!("loading {}: {e}", root.display()))
}

fn day(world: &World, year: i64, month: u8, day: u16) -> Day {
    world.calendar.to_day(CivilDate::ymd(year, month, day)).unwrap()
}

fn vale(snapshot: &SnapshotDto) -> &worldbuilder_lib::commands::EntityDto {
    snapshot
        .entities
        .iter()
        .find(|e| e.id == "ter_vale_of_corrath")
        .expect("the Vale should be present")
}

#[test]
fn a_settled_border_arrives_as_one_claim_with_a_colour() {
    let world = vashen();
    let snapshot = SnapshotDto::of(&world, day(&world, 700, 1, 1));
    let vale = vale(&snapshot);

    assert_eq!(vale.claims.len(), 1);
    assert_eq!(vale.claims[0].owner, "pol_corrath");
    assert_eq!(vale.claims[0].name, "The Duchy of Corrath");
    assert_eq!(vale.claims[0].color.as_deref(), Some("#B07A2B"));
    assert_eq!(vale.claims[0].certainty, "yes");

    // Geometry rides along, so the map needs no second round trip.
    assert_eq!(vale.shape.len(), 6);
}

#[test]
fn the_owner_has_changed_by_the_far_side_of_the_siege() {
    let world = vashen();
    let snapshot = SnapshotDto::of(&world, day(&world, 830, 1, 1));
    let vale = vale(&snapshot);

    assert_eq!(vale.claims.len(), 1);
    assert_eq!(vale.claims[0].owner, "pol_vashen");
    assert_eq!(vale.claims[0].color.as_deref(), Some("#2E7D6E"));
    assert_eq!(vale.claims[0].certainty, "yes");
}

/// The payload that makes the map draw a hatched, dashed border instead of picking.
#[test]
fn the_contested_month_sends_both_claims_as_possible() {
    let world = vashen();
    let snapshot = SnapshotDto::of(&world, day(&world, 812, 4, 15));
    let vale = vale(&snapshot);

    assert_eq!(vale.claims.len(), 2, "both claimants are live inside the doubt");
    assert!(vale.claims.iter().all(|c| c.certainty == "maybe"));

    let colors: Vec<_> = vale.claims.iter().filter_map(|c| c.color.as_deref()).collect();
    assert_eq!(colors.len(), 2, "the hatch needs two distinct colours");
    assert_ne!(colors[0], colors[1]);
}

#[test]
fn markers_and_labels_come_through() {
    let world = vashen();
    let snapshot = SnapshotDto::of(&world, day(&world, 812, 4, 15));

    let marrow = snapshot.entities.iter().find(|e| e.id == "place_marrow").unwrap();
    assert_eq!(marrow.marker, Some([0.43, 0.40]));
    assert_eq!(marrow.primitive, Some("place"));
    assert_eq!(snapshot.label, "15 Verdant, 813 AR");
}

/// Aldric died "around 811" and the siege is 812, so he is neither clearly alive nor
/// clearly dead. The bridge must carry that through rather than resolving it.
#[test]
fn uncertain_existence_survives_the_bridge() {
    let world = vashen();
    let snapshot = SnapshotDto::of(&world, day(&world, 812, 4, 15));

    let aldric = snapshot.entities.iter().find(|e| e.id == "act_aldric_vane").unwrap();
    assert_eq!(aldric.existence, "maybe");
    assert!(aldric.facts.iter().any(|f| f.attr == "title" && f.certainty == "maybe"));
}

#[test]
fn the_dead_are_simply_absent() {
    let world = vashen();
    let snapshot = SnapshotDto::of(&world, day(&world, 900, 1, 1));
    assert!(
        !snapshot.entities.iter().any(|e| e.id == "act_aldric_vane"),
        "the map should not be asked to draw someone a century dead"
    );
}

#[test]
fn the_summary_spans_past_every_change_point() {
    let world = vashen();
    let summary = WorldSummary::of(&world);

    assert_eq!(summary.entity_count, 12);
    assert_eq!(summary.event_count, 3);
    assert_eq!(summary.months.len(), 12);
    assert!(summary.undeclared_types.is_empty());

    let first = *summary.change_points.first().unwrap();
    let last = *summary.change_points.last().unwrap();
    assert!(summary.span[0] < first, "the track needs room before the first change");
    assert!(summary.span[1] > last, "and after the last");
}

/// One namespace, three primitives. The go-to box and the id collision check both read
/// this, and both are wrong the moment it is filtered by anything.
#[test]
fn every_record_in_the_world_is_addressable_whatever_kind_it_is() {
    let world = vashen();
    let summary = WorldSummary::of(&world);

    let kinds = |k: &str| summary.records.iter().filter(|r| r.kind == k).count();
    assert_eq!(kinds("entity"), summary.entity_count);
    assert_eq!(kinds("event"), summary.event_count);
    assert_eq!(kinds("scene"), summary.scene_count);
    assert_eq!(summary.records.len(), 12 + 3 + 3);

    let ids: BTreeSet<&str> = summary.records.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids.len(), summary.records.len(), "ids are unique across all three");

    let aldric = summary.records.iter().find(|r| r.id == "act_aldric_vane").unwrap();
    assert_eq!(aldric.name, "Aldric Vane");
    assert_eq!(aldric.type_name, "noble");
    assert_eq!(aldric.aka, ["Aldric", "the duke"], "what the prose calls him, to search on");

    let siege = summary.records.iter().find(|r| r.id == "evt_siege_of_marrow").unwrap();
    assert_eq!(siege.type_name, "conquest", "an event sends its kind where a type would be");

    let scene = summary.records.iter().find(|r| r.kind == "scene").unwrap();
    assert_eq!(scene.type_name, "", "and a scene has neither, and says so");
}

/// The vocabulary the form offers back. Counted by record, not by fact — Marrow's
/// population is asserted over three windows and is still one world that tracks it.
#[test]
fn the_attribute_vocabulary_is_ordered_by_how_much_of_the_world_uses_it() {
    let world = vashen();
    let summary = WorldSummary::of(&world);

    let counts: Vec<(usize, &str)> =
        summary.attrs.iter().map(|a| (a.count, a.name.as_str())).collect();
    assert!(counts.windows(2).all(|w| w[0].0 >= w[1].0), "most-used first: {counts:?}");

    let population = summary.attrs.iter().find(|a| a.name == "population").unwrap();
    let windows =
        world.entities["place_marrow"].facts.iter().filter(|f| f.attr == "population").count();
    assert!(windows > 1, "the fixture is what we think it is: {windows} windows");
    assert_eq!(population.count, 1, "one record tracks it, however many times it changed");

    assert!(summary.attrs.iter().any(|a| a.name == "owner"), "the map's own attribute");
    assert!(!summary.attrs.iter().any(|a| a.name.is_empty()), "nothing nameless gets offered");
}

/// The other half of a record: not what it says, but what says it.
#[test]
fn what_points_at_a_record_calls_every_kind_of_pointer_by_its_name() {
    let world = vashen();
    let pointing = references_of(&world, "act_aldric_vane");

    let named: Vec<(&str, &str, &str)> =
        pointing.iter().map(|r| (r.by.as_str(), r.name.as_str(), r.how)).collect();
    assert_eq!(
        named,
        [
            ("evt_oath_of_vashen", "The Oath of Vashen", "participant"),
            ("evt_siege_of_marrow", "The Siege of Marrow", "participant"),
            // The one that used to arrive as `scn_gate_at_dusk`: the name lookup knew
            // about entities and events, and a scene fell through to its own id.
            ("scn_gate_at_dusk", "The gate at dusk", "pov"),
        ]
    );
}

/// The state a delete leaves behind, which is the state this question is worth the most
/// in: the record is gone, three things still name it, and the panel can say which.
#[test]
fn what_pointed_at_a_record_survives_the_record() {
    let world = vashen();
    let after = world.without("act_aldric_vane").expect("nothing dates itself against Aldric");

    assert!(!after.knows("act_aldric_vane"), "the fixture is what we think it is");
    let orphaned: Vec<String> =
        references_of(&after, "act_aldric_vane").into_iter().map(|r| r.by).collect();
    assert_eq!(orphaned, ["evt_oath_of_vashen", "evt_siege_of_marrow", "scn_gate_at_dusk"]);
}

/// The scrubber's premise, checked against the payload the UI compares: sampling every
/// few days across three centuries yields only a handful of distinct renderings.
#[test]
fn scrubbing_across_centuries_yields_few_distinct_renderings() {
    let world = vashen();
    let from = day(&world, 600, 1, 1).0;
    let to = day(&world, 900, 1, 1).0;

    let mut seen = std::collections::BTreeSet::new();
    let mut samples = 0;
    for d in (from..to).step_by(5) {
        let snapshot = SnapshotDto::of(&world, Day(d));
        let shape: Vec<String> = snapshot
            .entities
            .iter()
            .map(|e| {
                let claims: Vec<&str> = e.claims.iter().map(|c| c.owner.as_str()).collect();
                format!("{}:{}:{}", e.id, e.existence, claims.join("+"))
            })
            .collect();
        seen.insert(shape);
        samples += 1;
    }

    assert!(samples > 20_000, "expected a dense sweep, got {samples}");

    // The claim is a ratio, not a magic number: three centuries of scrubbing should
    // collapse to a handful of renderings, so the map can skip almost every requery.
    let distinct = seen.len();
    assert!(
        samples / distinct > 500,
        "{samples} scrub positions produced {distinct} distinct renderings \
         (1 per {}); the change-point optimisation only pays off while that ratio \
         stays large",
        samples / distinct
    );
    assert!(distinct < 40, "unexpectedly many distinct renderings: {distinct}");
}
