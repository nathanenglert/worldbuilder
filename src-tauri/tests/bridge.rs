//! What the UI actually receives.
//!
//! The map's whole job is to render these payloads, so the payloads are where the
//! rendering decisions get checked — which polity holds a region, what colour that is,
//! and whether the claim is settled enough to draw a solid border.

use std::path::PathBuf;

use wb_core::{CivilDate, Day};
use wb_store::{World, load};
use worldbuilder_lib::commands::{SnapshotDto, WorldSummary};

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
