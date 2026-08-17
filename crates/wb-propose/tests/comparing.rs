//! Two worlds, compared in records.
//!
//! Every test here runs against the real example world rather than a fixture. A
//! comparison that is confidently wrong is worse than no comparison at all — a writer
//! discards an experiment on the strength of it — and the only way that stays true is if
//! the arithmetic is checked against a world with fuzzy dates, anchored facts and a
//! manuscript in it, not against three synthetic records that all resolve exactly.

use std::path::PathBuf;

use wb_propose::diff_worlds;
use wb_store::{World, load};

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/vashen")
}

fn vashen() -> World {
    load(example_root()).expect("load example world")
}

#[test]
fn comparing_a_world_to_itself_reports_no_change_at_all() {
    let world = vashen();
    let diff = diff_worlds(&world, &world);

    assert!(
        diff.is_empty(),
        "added {:?} removed {:?} changed {:?}",
        diff.added,
        diff.removed,
        diff.changed
    );
    assert!(diff.impact.resolved.is_empty(), "and settles nothing");
    assert!(diff.impact.introduced.is_empty(), "and breaks nothing");
}

/// Two worlds loaded from two different directories are still the same world. This is
/// the property the whole comparison rests on, because one side always arrives from a
/// scratch directory — and `source` paths differ on every single record.
#[test]
fn a_world_loaded_from_somewhere_else_is_not_reported_as_entirely_changed() {
    let world = vashen();
    let copy = load(example_root().canonicalize().expect("canonical")).expect("load again");
    assert!(diff_worlds(&world, &copy).is_empty(), "file paths are not a difference");
}

#[test]
fn a_new_record_is_reported_as_added_and_not_as_a_change() {
    let world = vashen();
    let mut entity = world.entities["act_isolde_corr"].clone();
    entity.id = "act_someone_new".to_string();
    entity.name = "Someone New".to_string();
    let after = world.with_entity(entity).expect("assembles");

    let diff = diff_worlds(&world, &after);
    assert_eq!(diff.counts(), (1, 0, 0));
    assert_eq!(diff.added[0].id, "act_someone_new");
    assert_eq!(diff.added[0].kind, "entity");
}

#[test]
fn a_deleted_record_is_reported_as_removed_with_the_name_it_had() {
    let world = vashen();
    let after = world.without("scn_the_breach").expect("assembles");

    let diff = diff_worlds(&world, &after);
    assert_eq!(diff.counts(), (0, 1, 0));
    assert_eq!(diff.removed[0].kind, "scene");
    assert_eq!(diff.removed[0].name, "The breach");
}

#[test]
fn a_changed_record_names_the_field_and_not_the_line() {
    let world = vashen();
    let mut marrow = world.entities["place_marrow"].clone();
    marrow.facts[0].value = wb_store::Value::Int(12_000);
    marrow.aliases.push("the wall town".to_string());
    let after = world.with_entity(marrow).expect("assembles");

    let diff = diff_worlds(&world, &after);
    assert_eq!(diff.counts(), (0, 0, 1));
    let changed = &diff.changed[0];
    assert_eq!(changed.id, "place_marrow");
    assert_eq!(changed.fields, ["aka +1", "facts +1 −1"]);
    assert!(changed.moved.is_empty(), "no date moved");
}

/// The consequence a line diff cannot show. `pol_corrath`'s existence ends
/// `@evt_siege_of_marrow`; nothing in its own file changes when the siege moves, but the
/// duchy now falls two years later than it did.
#[test]
fn a_record_whose_own_file_is_untouched_still_reports_the_date_that_moved() {
    let world = vashen();
    let mut siege = world.events["evt_siege_of_marrow"].clone();
    siege.date = wb_core::parse::parse_date("0814-04").expect("a date");
    let after = world.with_event(siege).expect("assembles");

    let diff = diff_worlds(&world, &after);

    let duchy = diff
        .changed
        .iter()
        .find(|c| c.id == "pol_corrath")
        .expect("the duchy is reported even though its file did not change");
    assert!(duchy.fields.is_empty(), "nothing in the record itself: {:?}", duchy.fields);
    let moved = &duchy.moved[0];
    assert_eq!(moved.what, "death");
    assert!(moved.days > 700, "two years later, give or take a calendar: {}", moved.days);

    let event = diff.changed.iter().find(|c| c.id == "evt_siege_of_marrow").expect("the siege");
    assert_eq!(event.fields, ["date"]);
    assert_eq!(event.moved[0].what, "date");
}

/// The same arithmetic the review queue shows, from the same function, so a branch and a
/// proposal never disagree about whether something is settled.
#[test]
fn the_findings_half_says_what_accepting_the_other_world_would_settle() {
    let world = vashen();
    let mut aldric = world.entities["act_aldric_vane"].clone();
    aldric.existence.as_mut().expect("Aldric has an existence").to =
        wb_core::parse::parse_date("@evt_siege_of_marrow+1y").expect("an anchor");
    let after = world.with_entity(aldric).expect("assembles");

    let diff = diff_worlds(&world, &after);
    assert_eq!(
        diff.impact.resolved.len(),
        2,
        "the same two open questions about Aldric — one from the record, one from \
         chapter twelve: {:?}",
        diff.impact.resolved.iter().map(|f| f.message.clone()).collect::<Vec<_>>()
    );
    assert!(!diff.impact.breaks_something());
}
