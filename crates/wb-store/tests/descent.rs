//! Lineage, and the two shapes of baton.
//!
//! Against the example world, because the interesting cases here are all consequences of
//! fuzzy dates and anchored facts: a tenure that ends `@act_maren_vane.death` and one
//! that begins on the same day meet exactly, and the pair of them has to come out as a
//! clean handover rather than as a hole or a double claim.

use std::path::PathBuf;

use wb_store::kin::{self, Kind};
use wb_store::{World, load};

fn vashen() -> World {
    load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/vashen"))
        .expect("load example world")
}

// -------------------------------------------------------------------- descent

#[test]
fn descent_walks_both_ways_from_the_same_parentage_edges() {
    let world = vashen();

    let up: Vec<(&str, usize)> = kin::ancestors(&world, "act_aldric_vane", 3)
        .iter()
        .map(|r| (r.entity.name.as_str(), r.generation))
        .collect();
    assert_eq!(up, [("Maren Vane", 1), ("Isolde Corr", 1), ("Aldric Vane III", 2)]);

    let down: Vec<(&str, usize)> = kin::descendants(&world, "act_aldric_vane_iii", 3)
        .iter()
        .map(|r| (r.entity.name.as_str(), r.generation))
        .collect();
    assert_eq!(
        down,
        [("Maren Vane", 1), ("Aldric Vane", 2)],
        "and children are found by looking, not by storing them"
    );

    assert!(kin::ancestors(&world, "act_aldric_vane_iii", 3).is_empty());
    assert!(kin::descendants(&world, "act_aldric_vane", 3).is_empty());
}

#[test]
fn a_generation_is_the_longest_path_up_and_not_the_shortest() {
    let world = vashen();
    assert_eq!(
        kin::generation_of(&world, "act_aldric_vane_iii"),
        0,
        "nobody's child, as far as this world knows"
    );
    assert_eq!(kin::generation_of(&world, "act_maren_vane"), 1);
    assert_eq!(
        kin::generation_of(&world, "act_isolde_corr"),
        0,
        "married in, and her own parents are not recorded"
    );
    assert_eq!(
        kin::generation_of(&world, "act_aldric_vane"),
        2,
        "two steps from the oldest Vane, not one from the nearest"
    );
    assert_eq!(kin::generation_of(&world, "place_marrow"), 0, "a city has no parents");
}

#[test]
fn a_house_is_a_connected_component_and_not_a_surname() {
    let world = vashen();
    let house: Vec<&str> =
        kin::house(&world, "act_aldric_vane").iter().map(|e| e.name.as_str()).collect();
    assert!(house.contains(&"Aldric Vane"));
    assert!(house.contains(&"Maren Vane"));
    assert!(
        house.contains(&"Isolde Corr"),
        "married in, different surname, same house — the edges decide: {house:?}"
    );
    assert!(!house.contains(&"Marrow"));

    assert_eq!(
        kin::house(&world, "act_maren_vane").len(),
        house.len(),
        "and it is the same house seen from anywhere in it"
    );
}

#[test]
fn a_record_nobody_is_related_to_is_a_house_of_one() {
    let world = vashen();
    let alone = kin::house(&world, "place_marrow");
    assert_eq!(alone.len(), 1);
    assert_eq!(alone[0].id, "place_marrow");
}

// ---------------------------------------------------------------- successions

#[test]
fn a_title_is_one_value_passed_between_records() {
    let world = vashen();
    let ducal = kin::successions(&world)
        .into_iter()
        .find(|s| s.label == "Duke of Corrath")
        .expect("the ducal title");

    assert_eq!(ducal.kind, Kind::Title);
    assert_eq!(ducal.attr, "title");
    assert_eq!(
        ducal.holder_ids(),
        ["act_aldric_vane_iii", "act_maren_vane", "act_aldric_vane"],
        "in the order they held it"
    );
}

/// The Vane line holds one handover of each kind, and they must not come out the same.
///
/// Maren's tenure ends `@act_maren_vane.death` and his son's begins the same day, because
/// `[from, to)` is half-open — reporting a one-day interregnum there would be reporting an
/// artefact of this function's own arithmetic. The handover *above* it is two dates both
/// written `0768~`, and those genuinely do overlap: nobody wrote the day down. Unsettled,
/// not contested, and the same reason the map hatches a border rather than picking a side.
#[test]
fn an_exact_handover_leaves_no_hole_and_a_vague_one_leaves_an_unsettled_stretch() {
    let world = vashen();
    let ducal = kin::successions(&world)
        .into_iter()
        .find(|s| s.label == "Duke of Corrath")
        .expect("the ducal title");

    assert!(ducal.gaps.is_empty(), "the Vale is never unruled: {:?}", ducal.gaps);
    assert_eq!(ducal.overlaps.len(), 1, "exactly the vague one: {:?}", ducal.overlaps);

    // And it is the earlier handover, not the one the chronicle pins to the day.
    let unsettled = ducal.overlaps[0];
    let maren_dies = world.lifespan("act_maren_vane").expect("Maren").possible.to.expect("dated");
    assert!(unsettled.to.expect("bounded") < maren_dies);
}

/// The Vale's `owner` is the other shape entirely — one record, one attribute, two
/// values — and it is the reason this is not restricted to actors and titles.
#[test]
fn succession_is_not_restricted_to_actors_or_to_titles() {
    let world = vashen();
    let vale = kin::successions(&world)
        .into_iter()
        .find(|s| s.of == "ter_vale_of_corrath")
        .expect("the Vale changes hands");

    assert_eq!(vale.kind, Kind::Office);
    assert_eq!(vale.attr, "owner");
    assert_eq!(vale.label, "The Vale of Corrath · owner");
    assert_eq!(vale.holder_ids(), ["pol_corrath", "pol_vashen"]);
    assert!(
        vale.holders
            .iter()
            .all(|t| world.primitive_of(t.holder) == Some(wb_store::Primitive::Polity)),
        "the holders are polities, which is exactly the point"
    );
}

#[test]
fn a_thing_only_one_record_ever_held_is_not_a_succession() {
    let world = vashen();
    let all = kin::successions(&world);

    assert!(
        !all.iter().any(|s| s.attr == "color"),
        "every polity has its own colour, and one holder is not a handover"
    );
    assert!(
        all.iter().all(|s| s.holders.len() >= 2),
        "nothing with a single holder gets in at all"
    );
}

#[test]
fn a_hole_between_two_tenures_is_reported_as_a_gap() {
    let world = vashen();
    let mut aldric = world.entities["act_aldric_vane"].clone();
    // Take the seat five years late, leaving the duchy's chair empty in between.
    aldric.facts[0].from = wb_core::parse::parse_date("0804-01-01").expect("a date");
    let world = world.with_entity(aldric).expect("assembles");

    let ducal = kin::successions(&world)
        .into_iter()
        .find(|s| s.label == "Duke of Corrath")
        .expect("the ducal title");
    assert_eq!(ducal.gaps.len(), 1, "one unruled stretch");
    let gap = ducal.gaps[0];
    assert!(gap.to.unwrap().0 - gap.from.unwrap().0 > 1_500, "about five years of it");
}

#[test]
fn two_holders_at_once_are_an_overlap_and_not_a_gap() {
    let world = vashen();
    let mut maren = world.entities["act_maren_vane"].clone();
    // Maren holds the title a decade past his son's accession: a contested claim, which
    // is a thing a world is allowed to say.
    maren.facts[0].to = wb_core::parse::parse_date("0809-01-01").expect("a date");
    let world = world.with_entity(maren).expect("assembles");

    let ducal = kin::successions(&world)
        .into_iter()
        .find(|s| s.label == "Duke of Corrath")
        .expect("the ducal title");
    assert!(ducal.gaps.is_empty());
    assert_eq!(
        ducal.overlaps.len(),
        2,
        "the vague handover in 768, and the ten deliberate years just added"
    );
}

#[test]
fn the_longest_chain_is_offered_first() {
    let world = vashen();
    let all = kin::successions(&world);
    assert!(!all.is_empty());
    for pair in all.windows(2) {
        assert!(pair[0].holders.len() >= pair[1].holders.len(), "sorted by how much has passed");
    }
}
