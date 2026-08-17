//! The manuscript read against the shipped example world.
//!
//! Deliberately measured against `examples/vashen` and its real two-chapter manuscript
//! rather than synthetic fixtures, for the reason the writer's tests give: the failure
//! this code exists to prevent is not "the algorithm is wrong in the abstract" but "the
//! number shown to a writer was wrong", and only real prose can fail that way.

use std::path::PathBuf;

use wb_check::{Certainty, Rule};
use wb_store::{World, load};
use wb_story::iceberg::{self, Quadrant};

fn vashen() -> World {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/vashen");
    load(&root).unwrap_or_else(|e| panic!("loading {}: {e}", root.display()))
}

fn story(world: &World) -> wb_story::Story {
    wb_story::Story::read(world)
}

#[test]
fn every_scene_in_the_example_world_finds_its_prose() {
    let world = vashen();
    let story = story(&world);

    assert_eq!(story.standing(&world), wb_story::Standing::Linked);
    let (read, missing) = story.counts();
    assert_eq!((read, missing), (3, 0), "three scenes, every link resolving");

    let breach = story.get("scn_the_breach").expect("the scene is there");
    let passage = breach.passage.as_ref().expect("its prose reads");
    assert_eq!(passage.file, "ch12-the-siege.md");
    assert_eq!(passage.heading.as_deref(), Some("The breach"));
    assert!(passage.text.contains("the wall of Marrow opened"));
    assert!(!passage.text.contains("Twelve — The Siege"), "the anchor narrows to its section");
}

/// Reading order is the manuscript's, not the calendar's. Chapter one holds a flashback,
/// so the story path and the timeline disagree — and both are right.
#[test]
fn reading_order_is_the_books_order_and_not_chronological() {
    let world = vashen();
    let story = story(&world);

    let by_reading: Vec<&str> = story.reads.iter().map(|r| r.scene.as_str()).collect();
    assert_eq!(
        by_reading,
        ["scn_gate_at_dusk", "scn_word_from_the_vale", "scn_the_breach"],
        "chapter one's two scenes in the order they are written, then chapter twelve"
    );

    let day = |id: &str| world.resolved_node(id).and_then(|r| r.nominal).unwrap();
    assert!(
        day("scn_word_from_the_vale") < day("scn_gate_at_dusk"),
        "and the second scene read is the earliest one set — that is the flashback"
    );
}

/// The headline number, and every record behind it.
#[test]
fn the_iceberg_counts_what_the_prose_actually_names() {
    let world = vashen();
    let report = iceberg::report(&world, &story(&world));

    assert_eq!(report.total, 11);
    assert_eq!(report.surfaced, 8);
    assert_eq!(report.ratio(), Some(73));

    let by_id = |id: &str| report.entries.iter().find(|e| e.id == id).expect(id);

    // Named by their full names in the prose.
    assert!(by_id("place_marrow").mentions >= 3);
    assert!(by_id("ter_vale_of_corrath").mentions >= 3);

    // Named only by alias. Without `aka` both of these would read as never appearing.
    assert!(by_id("act_maren_vane").mentions > 0, "the book only ever says `Maren`");
    assert!(by_id("pol_vashen").mentions > 0, "and only ever says `Vashen`");

    // Genuinely absent from the book.
    assert_eq!(by_id("thing_high_tongue").mentions, 0);
    assert_eq!(by_id("act_isolde_corr").mentions, 0);
    assert!(!by_id("thing_high_tongue").surfaced());
}

/// Every mention carries the sentence it came from, because a number a writer will act on
/// has to be checkable rather than trusted.
#[test]
fn every_surfaced_record_can_show_the_sentence_that_counted() {
    let world = vashen();
    let report = iceberg::report(&world, &story(&world));

    for entry in report.entries.iter().filter(|e| e.surfaced()) {
        let line = entry.first_seen.as_ref().unwrap_or_else(|| {
            panic!("{} is counted as surfaced with nothing to show for it", entry.id)
        });
        assert!(!line.trim().is_empty(), "{} has an empty excerpt", entry.id);
    }
}

/// What a scene *claims* and what its page *says* are different measurements, and the
/// report keeps them in different columns.
///
/// The case that matters is a record listed in a scene's cast that never appears in the
/// prose — a note to self that the chapter outgrew, or a name the draft cut. Counting it
/// as surfaced would be the report telling a writer their world reaches a page it does
/// not reach, which is the one thing this feature must not do.
#[test]
fn a_name_in_a_scenes_cast_that_the_page_never_says_is_not_surfaced() {
    let world = vashen();

    let mut scene = world.scenes["scn_gate_at_dusk"].clone();
    scene.on_page.push("thing_high_tongue".into()); // named nowhere in chapter one
    let claimed = world.with_scene(scene).expect("assembles");

    let report = iceberg::report(&claimed, &story(&claimed));
    let tongue = report.entries.iter().find(|e| e.id == "thing_high_tongue").unwrap();

    assert_eq!(tongue.cast_in, 1, "the record says it is in the scene");
    assert_eq!(tongue.mentions, 0, "the prose never says so");
    assert!(!tongue.surfaced(), "and the page is what decides");

    // Where both are true, both are counted — and they need not agree in number.
    let vale = report.entries.iter().find(|e| e.id == "ter_vale_of_corrath").unwrap();
    assert!(vale.cast_in > 0 && vale.mentions > vale.cast_in);
}

/// The rule DESIGN.md §5 deferred. Chapter twelve names Aldric at the siege; his death is
/// `0811~` and the siege is `0812-04~`, so both readings survive and the finding is a
/// question rather than an error — which is the whole point of the certainty split.
#[test]
fn prose_naming_someone_who_may_be_dead_is_a_question_not_an_error() {
    let world = vashen();
    let findings = wb_story::canon::check(&world, &story(&world));

    assert_eq!(findings.len(), 1, "exactly one, and it is the deliberate one");
    let f = &findings[0];
    assert_eq!(f.rule, Rule::SceneContradiction);
    assert_eq!(f.certainty, Certainty::Possible);
    assert_eq!(f.subject, "scn_the_breach");
    assert_eq!(f.related, vec!["act_aldric_vane".to_string()]);
    assert!(f.message.contains("permits but does not confirm"));
    assert!(f.sources.iter().any(|p| p.ends_with("the-breach.yaml")));
}

/// A world that has not linked a manuscript is not a broken world. It reports every
/// record below the waterline, which is exactly true, and nothing anywhere errors.
#[test]
fn a_world_with_no_manuscript_is_fully_submerged_rather_than_an_error() {
    let mut world = vashen();
    world.manuscript = None;

    let story = story(&world);
    assert_eq!(story.standing(&world), wb_story::Standing::Unlinked);
    assert_eq!(story.counts(), (0, 0));

    let report = iceberg::report(&world, &story);
    assert_eq!(report.surfaced, 0);
    assert_eq!(report.ratio(), Some(0));
    assert!(report.entries.iter().all(|e| !e.surfaced()));
    assert!(wb_story::canon::check(&world, &story).is_empty());
}

/// A declared root that is not there is reported, not thrown.
#[test]
fn a_manuscript_root_that_moved_is_reported_rather_than_fatal() {
    let mut world = vashen();
    world.manuscript.as_mut().unwrap().root = PathBuf::from("../nowhere-at-all");

    let story = story(&world);
    assert_eq!(story.standing(&world), wb_story::Standing::RootMissing);

    let report = iceberg::report(&world, &story);
    assert_eq!(report.unreadable.len(), 3, "each scene says so for itself");
    assert!(
        report.unreadable.iter().all(|(_, why)| why.contains("manuscript root")),
        "and says which folder is missing: {:?}",
        report.unreadable
    );
    assert_eq!(report.surfaced, 0);
}

/// The link is the one path allowed out of the world folder, and it is still a leash.
#[test]
fn a_link_that_climbs_out_of_the_manuscript_root_is_refused() {
    let world = vashen();
    let base = wb_story::manuscript::root(&world).unwrap();

    let err = wb_story::manuscript::read(&base, "../vashen/world.yaml")
        .expect_err("climbing out is refused");
    assert!(err.contains("outside the manuscript root"), "got: {err}");

    let err = wb_story::manuscript::read(&base, "/etc/hosts").expect_err("absolute is refused");
    assert!(err.contains("absolute"), "got: {err}");
}

/// A link pointing at a real file but a heading that is not in it fails loudly. Falling
/// back to the whole chapter would silently inflate every count in the report.
#[test]
fn an_anchor_that_matches_nothing_does_not_quietly_become_the_whole_chapter() {
    let world = vashen();
    let base = wb_story::manuscript::root(&world).unwrap();

    let err = wb_story::manuscript::read(&base, "ch12-the-siege.md#the-retreat")
        .expect_err("the anchor is refused");
    assert!(err.contains("no heading matching"), "got: {err}");

    let whole = wb_story::manuscript::read(&base, "ch12-the-siege.md").expect("no anchor is fine");
    assert!(whole.text.contains("Twelve — The Siege"));
    assert!(whole.words > 100);
}

/// Scenes are dated records naming records, so the rule that catches an event with a dead
/// participant catches a scene with a dead point-of-view character, with no new rule.
#[test]
fn a_scene_whose_pov_was_not_alive_is_caught_by_the_rule_that_already_existed() {
    let world = vashen();
    let mut scene = world.scenes["scn_the_breach"].clone();
    scene.pov = Some("act_maren_vane".into()); // she dies 0799; the siege is 0812

    let broken = world.with_scene(scene).expect("the world still assembles");
    let report = wb_check::check(&broken);

    let found: Vec<_> = report
        .of_rule(Rule::ExistenceViolation)
        .filter(|f| f.subject == "scn_the_breach")
        .collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].certainty, Certainty::Definite, "both dates are exact — no reading works");
    assert!(found[0].message.contains("point of view"), "got: {}", found[0].message);
}

/// And a typo in a scene's cast is as much a dangling reference as one in an event's.
#[test]
fn a_scene_naming_an_id_nothing_defines_is_an_orphan_reference() {
    let world = vashen();
    let mut scene = world.scenes["scn_gate_at_dusk"].clone();
    scene.on_page.push("act_aldric_vaen".into());

    let broken = world.with_scene(scene).expect("assembles");
    let found: Vec<_> = wb_check::check(&broken)
        .of_rule(Rule::OrphanReference)
        .filter(|f| f.subject == "scn_gate_at_dusk")
        .cloned()
        .collect();

    assert_eq!(found.len(), 1);
    assert!(found[0].message.contains("act_aldric_vaen"));
}

/// The quadrants, on numbers chosen rather than measured — the example world has no
/// underbuilt record, and a heuristic still has to be shown to sort one when there is one.
#[test]
fn a_record_the_story_leans_on_with_nothing_in_it_is_reported_as_underbuilt() {
    let world = vashen();

    // Give Marrow's prose and facts away, leaving the mentions and references intact.
    let mut hollow = world.entities["place_marrow"].clone();
    hollow.facts.clear();
    hollow.body.clear();
    let thinned = world.with_entity(hollow).expect("assembles");

    let report = iceberg::report(&thinned, &story(&thinned));
    let marrow = report.entries.iter().find(|e| e.id == "place_marrow").unwrap();

    assert_eq!(marrow.quadrant, Quadrant::Underbuilt);
    assert!(marrow.mentions > 0, "still all over the page");
    assert_eq!(marrow.facts, 0, "with nothing behind it");

    // And underbuilt sorts first, because that is the report's opinion.
    assert_eq!(report.entries[0].quadrant, Quadrant::Underbuilt);
}
