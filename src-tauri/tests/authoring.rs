//! What the app does when the writer saves.
//!
//! These drive the plain functions in `edit.rs` rather than the `#[tauri::command]`
//! wrappers, which need a `State` — the same seam `bridge.rs` uses. The wrappers are
//! three lines each and the interesting behaviour is all below them.
//!
//! Every test that writes works on a throwaway copy of the example world. A test that
//! left a change behind would alter the answers of every test after it, here and in
//! `bridge.rs`, which asserts the shipped world still has eleven entities.

use std::fs;
use std::path::{Path, PathBuf};

use wb_store::{World, load};
use worldbuilder_lib::edit::{
    EntityDraft, EventDraft, FactDraft, commit, plan_delete, plan_entity, plan_event,
};
use worldbuilder_lib::story::{SceneDraft, SceneRecordDto, plan_scene};

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/vashen")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), &dest).unwrap();
        }
    }
}

/// A throwaway world, with the manuscript beside it where `../manuscript` expects.
///
/// The nesting matters: the example world's `manuscript.root` climbs out of its own
/// folder, which is the point of declaring it. A scratch copy that dropped the sibling
/// would test a world whose book had gone missing, and quietly stop exercising anything
/// the story layer does.
fn scratch(tag: &str) -> PathBuf {
    let parent = std::env::temp_dir().join(format!("wb-authoring-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&parent);
    let dir = parent.join("vashen");
    copy_dir(&example_root(), &dir);
    copy_dir(&example_root().join("../manuscript"), &parent.join("manuscript"));
    dir
}

fn open(root: &Path) -> World {
    load(root).unwrap_or_else(|e| panic!("loading {}: {e}", root.display()))
}

/// Marrow, as a draft that changes nothing.
fn marrow_draft(world: &World) -> EntityDraft {
    draft_of(world, "place_marrow")
}

/// Any record, as the draft a form would hold if it rendered every field.
fn draft_of(world: &World, id: &str) -> EntityDraft {
    let e = &world.entities[id];
    EntityDraft {
        id: e.id.clone(),
        name: e.name.clone(),
        aka: Some(e.aliases.clone()),
        type_name: e.type_name.clone(),
        existence_from: e.existence.as_ref().map(|s| s.from.to_string()),
        existence_to: e.existence.as_ref().and_then(|s| match &s.to {
            wb_core::DateExpr::Unknown => None,
            other => Some(other.to_string()),
        }),
        parents: Some(e.parents.clone()),
        facts: e
            .facts
            .iter()
            .map(|f| FactDraft {
                attr: f.attr.clone(),
                value: f.value.clone(),
                from: match &f.from {
                    wb_core::DateExpr::Unknown => None,
                    d => Some(d.to_string()),
                },
                to: match &f.to {
                    wb_core::DateExpr::Unknown => None,
                    d => Some(d.to_string()),
                },
            })
            .collect(),
        marker: e.marker,
        shape: e.shape.clone(),
        body: None,
    }
}

// ---- the round trip that matters most

/// The trap this whole layer exists to avoid.
///
/// `SnapshotDto` — what the map already receives — has no `from`/`to` on a fact and
/// stringifies every value through `Display`. A form bound to *that* and saved back
/// would resolve `to: "@evt_siege_of_marrow"` into nothing and turn `9000` into `"9000"`,
/// silently and irreversibly. So: load, hand the record to the editor, save it back
/// unchanged, and require the file to be untouched.
#[test]
fn a_record_that_goes_out_to_the_editor_and_straight_back_changes_nothing() {
    let root = scratch("round-trip");
    let world = open(&root);
    let path = root.join("entities/places/marrow.md");
    let before = fs::read_to_string(&path).unwrap();

    let plan = plan_entity(&world, marrow_draft(&world)).expect("plans");
    commit(&plan, plan.revision.as_deref(), false).expect("commits");

    assert_eq!(fs::read_to_string(&path).unwrap(), before, "a no-op save rewrote the file");

    let reloaded = open(&root);
    let marrow = &reloaded.entities["place_marrow"];
    assert_eq!(marrow.facts[0].value, wb_store::Value::Int(9000), "the number stayed a number");
    assert_eq!(
        marrow.facts[0].to.to_string(),
        "@evt_siege_of_marrow",
        "the anchor stayed an anchor"
    );
    assert!(marrow.body.contains("wall town"), "the prose is still there");
}

/// The trap the round-trip above cannot see, because it builds its payload in Rust.
///
/// The form sends JSON, and for a long time it sent JSON with no `aka` and no `parents`
/// in it at all. `#[serde(default)]` turned both absences into empty vectors and the
/// write went through unconditionally, so editing Aldric's name deleted the two names
/// the manuscript scanner matches him by and both of his parentage edges — the whole of
/// what the lineage view draws. Nothing failed; the record simply came back smaller.
///
/// So this test deserializes rather than constructs, and the payload below is deliberately
/// missing both keys.
#[test]
fn a_payload_that_never_mentioned_the_aliases_leaves_them_where_they_were() {
    let root = scratch("absent-aka");
    let world = open(&root);
    let before = &world.entities["act_aldric_vane"];
    assert_eq!(before.aliases, ["Aldric", "the duke"], "the fixture is what we think it is");
    assert_eq!(before.parents, ["act_maren_vane", "act_isolde_corr"]);

    let draft: EntityDraft = serde_json::from_value(serde_json::json!({
        "id": "act_aldric_vane",
        "name": "Aldric Vane the Younger",
        "type": "noble",
        "existence_from": "0771-06-12",
        "existence_to": "0811~",
        "facts": [],
        "marker": null,
        "shape": [],
        "body": null,
    }))
    .expect("the shape the form sends");

    let plan = plan_entity(&world, draft).expect("plans");
    commit(&plan, plan.revision.as_deref(), true).expect("commits");

    let after = open(&root);
    let aldric = &after.entities["act_aldric_vane"];
    assert_eq!(aldric.name, "Aldric Vane the Younger", "the edit that was asked for happened");
    assert_eq!(aldric.aliases, ["Aldric", "the duke"], "and nothing else did");
    assert_eq!(aldric.parents, ["act_maren_vane", "act_isolde_corr"]);
}

/// The other half of the same contract: a form that *did* render the box must be able to
/// empty it. Otherwise an alias typed by mistake could never be taken back.
#[test]
fn a_payload_that_sends_an_empty_alias_list_really_does_clear_it() {
    let root = scratch("cleared-aka");
    let world = open(&root);

    let mut draft = draft_of(&world, "act_aldric_vane");
    draft.aka = Some(Vec::new());
    draft.parents = Some(vec!["  ".into(), "act_maren_vane".into()]);

    let plan = plan_entity(&world, draft).expect("plans");
    commit(&plan, plan.revision.as_deref(), true).expect("commits");

    let aldric = &open(&root).entities["act_aldric_vane"];
    assert!(aldric.aliases.is_empty(), "an emptied box means empty");
    assert_eq!(aldric.parents, ["act_maren_vane"], "and a blank row is not a reference");
}

// ---- saving

#[test]
fn saving_an_edited_record_writes_the_file_and_the_world_reloads() {
    let root = scratch("save");
    let world = open(&root);

    let mut draft = marrow_draft(&world);
    draft.facts[0].value = wb_store::Value::Int(9500);

    let plan = plan_entity(&world, draft).expect("plans");
    commit(&plan, plan.revision.as_deref(), false).expect("commits");

    let reloaded = open(&root);
    assert_eq!(reloaded.entities["place_marrow"].facts[0].value, wb_store::Value::Int(9500));
}

#[test]
fn saving_a_marker_changes_only_the_marker_line() {
    let root = scratch("marker");
    let world = open(&root);
    let path = root.join("entities/places/marrow.md");
    let before = fs::read_to_string(&path).unwrap();

    let mut draft = marrow_draft(&world);
    draft.marker = Some([0.5, 0.25]);

    let plan = plan_entity(&world, draft).expect("plans");
    commit(&plan, plan.revision.as_deref(), false).expect("commits");

    let after = fs::read_to_string(&path).unwrap();
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    assert_eq!(old.len(), new.len());
    let changed: Vec<usize> = (0..old.len()).filter(|&i| old[i] != new[i]).collect();
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert!(new[changed[0]].contains("0.5"));
}

#[test]
fn a_preview_touches_nothing_on_disk() {
    let root = scratch("preview");
    let world = open(&root);
    let path = root.join("entities/places/marrow.md");
    let before = fs::read_to_string(&path).unwrap();

    let mut draft = marrow_draft(&world);
    draft.name = "Somewhere Else".into();
    let plan = plan_entity(&world, draft).expect("plans");

    assert!(!plan.files.is_empty(), "there is something to write");
    assert_eq!(fs::read_to_string(&path).unwrap(), before, "but nothing was written");
}

/// Impact before commit, which is the thing the panel refuses to let you skip.
#[test]
fn a_preview_shows_the_contradiction_a_save_would_introduce() {
    let root = scratch("impact");
    let world = open(&root);

    // Marrow cannot be a wall town founded after the siege that levelled it.
    let mut draft = marrow_draft(&world);
    draft.existence_from = Some("0900".into());

    let plan = plan_entity(&world, draft).expect("plans");
    assert!(
        plan.impact.breaks_something() || !plan.impact.introduced.is_empty(),
        "moving a town's founding past the events it takes part in should be noticed: {:#?}",
        plan.impact
    );
}

// ---- the guards

#[test]
fn a_save_whose_file_changed_underneath_is_refused() {
    let root = scratch("stale");
    let world = open(&root);
    let path = root.join("entities/places/marrow.md");

    let mut draft = marrow_draft(&world);
    draft.name = "Marrow-on-the-Wall".into();
    let plan = plan_entity(&world, draft).expect("plans");

    // Somebody else — the writer's own editor, say — saves in between.
    let meanwhile = fs::read_to_string(&path).unwrap().replace("type: city", "type: town");
    fs::write(&path, &meanwhile).unwrap();

    let err = commit(&plan, plan.revision.as_deref(), false)
        .expect_err("the file moved on and the save should say so");
    assert!(err.contains("changed on disk"), "got {err}");
    assert_eq!(fs::read_to_string(&path).unwrap(), meanwhile, "and nothing was overwritten");
}

#[test]
fn creating_an_entity_with_an_id_that_already_exists_is_refused() {
    let root = scratch("collision");
    let world = open(&root);

    let mut draft = marrow_draft(&world);
    // Entities and events share one id namespace, so this is a clash even though no
    // entity holds the id.
    draft.id = "evt_siege_of_marrow".into();

    let err = plan_entity(&world, draft).expect_err("an event already owns that id");
    assert!(err.contains("evt_siege_of_marrow"), "got {err}");
}

#[test]
fn a_bad_date_comes_back_naming_the_field_that_carried_it() {
    let root = scratch("bad-date");
    let world = open(&root);

    let mut draft = marrow_draft(&world);
    draft.facts[0].from = Some("0812~~".into());

    let err = plan_entity(&world, draft).expect_err("that is not a date");
    assert!(err.contains("from"), "the message should name the field: {err}");
    assert!(err.contains("0812~~"), "and quote what was typed: {err}");
}

/// An empty box means `?`, and `?` is a perfectly good answer. A form that rendered two
/// existence fields must not thereby give a record dates it never had.
#[test]
fn leaving_the_dates_empty_does_not_invent_an_existence() {
    let root = scratch("no-dates");
    let world = open(&root);

    let draft = EntityDraft {
        id: "place_greyford".into(),
        name: "Greyford".into(),
        aka: Some(Vec::new()),
        type_name: "city".into(),
        existence_from: None,
        existence_to: None,
        parents: Some(Vec::new()),
        facts: Vec::new(),
        marker: None,
        shape: Vec::new(),
        body: Some("A ford, and a name for it.".into()),
    };

    let plan = plan_entity(&world, draft).expect("plans");
    commit(&plan, None, false).expect("commits");

    let reloaded = open(&root);
    let made = &reloaded.entities["place_greyford"];
    assert!(made.existence.is_none(), "an unstated existence must stay unstated");
    assert!(made.facts.is_empty());
    assert!(made.body.contains("A ford"));
    assert!(root.join("entities/places/greyford.md").is_file(), "filed by its primitive");
}

// ---- events

#[test]
fn creating_an_event_places_it_by_year_and_reloads() {
    let root = scratch("event");
    let world = open(&root);

    let draft = EventDraft {
        id: "evt_relief_of_marrow".into(),
        name: "The Relief of Marrow".into(),
        kind: Some("battle".into()),
        date: "@evt_siege_of_marrow+1y".into(),
        participants: vec!["pol_corrath".into()],
        location: Some("place_marrow".into()),
    };

    let plan = plan_event(&world, draft).expect("plans");
    commit(&plan, None, false).expect("commits");

    let reloaded = open(&root);
    assert!(reloaded.events.contains_key("evt_relief_of_marrow"));
    assert_eq!(
        reloaded.events["evt_relief_of_marrow"].date.to_string(),
        "@evt_siege_of_marrow+1y",
        "the anchor is kept as an anchor, not resolved to a number"
    );
}

// ---- deleting

#[test]
fn deleting_a_record_says_what_still_points_at_it() {
    let root = scratch("delete-refs");
    let world = open(&root);

    let plan = plan_delete(&world, "place_marrow").expect("plans");
    assert!(
        plan.references.iter().any(|r| r.how == "location"),
        "the siege happens there: {:?}",
        plan.references
    );
}

/// The siege is what half the Vale's history is dated against, so removing it would
/// strand those anchors. Refusing is the answer; a world that no longer resolves is not.
#[test]
fn deleting_something_other_dates_depend_on_is_refused_with_a_reason() {
    let root = scratch("delete-anchored");
    let world = open(&root);

    let err = plan_delete(&world, "evt_siege_of_marrow").expect_err("refused");
    assert!(err.contains("dates itself against"), "got {err}");
}

#[test]
fn deleting_a_record_nothing_depends_on_removes_its_file() {
    let root = scratch("delete");
    let world = open(&root);
    let path = root.join("entities/things/high-tongue.md");
    assert!(path.is_file(), "the fixture is where we think it is");

    let plan = plan_delete(&world, "thing_high_tongue").expect("plans");
    commit(&plan, plan.revision.as_deref(), true).expect("commits");

    assert!(!path.exists(), "the file should be gone");
    assert!(!open(&root).entities.contains_key("thing_high_tongue"));
}

// ------------------------------------------------------------ scenes

fn breach_draft(world: &World) -> SceneDraft {
    let s = &world.scenes["scn_the_breach"];
    SceneDraft {
        id: s.id.clone(),
        name: s.name.clone(),
        date: s.date.to_string(),
        pov: s.pov.clone(),
        on_page: s.on_page.clone(),
        location: s.location.clone(),
        prose: s.prose.clone(),
    }
}

/// The scene equivalent of the slice-4.5 guarantee: a record that goes out to the editor
/// and comes straight back changes nothing at all, including the comment above it.
#[test]
fn a_scene_that_goes_out_to_the_editor_and_straight_back_changes_nothing() {
    let root = scratch("scene-roundtrip");
    let world = open(&root);
    let path = root.join("scenes/the-breach.yaml");
    let before = fs::read_to_string(&path).unwrap();

    let plan = plan_scene(&world, breach_draft(&world)).expect("plans");
    assert!(plan.preserves_bytes());
    commit(&plan, plan.revision.as_deref(), false).expect("commits");

    assert_eq!(fs::read_to_string(&path).unwrap(), before, "byte for byte");
    assert!(before.contains("# Anchored to the siege"), "and the comment was there to keep");
}

/// The anchor survives as an anchor. Resolving `@evt_siege_of_marrow` into a year on the
/// way through the form would silently unhook chapter twelve from the siege it depicts.
#[test]
fn a_scene_anchored_to_an_event_stays_anchored_through_an_edit() {
    let root = scratch("scene-anchor");
    let world = open(&root);

    let record = SceneRecordDto::of(&world, &world.scenes["scn_the_breach"]);
    assert_eq!(record.date, "@evt_siege_of_marrow", "it arrives as authored");

    let mut draft = breach_draft(&world);
    draft.name = "The wall opens".into();
    let plan = plan_scene(&world, draft).expect("plans");
    commit(&plan, plan.revision.as_deref(), false).expect("commits");

    let after = open(&root);
    assert_eq!(after.scenes["scn_the_breach"].date.to_string(), "@evt_siege_of_marrow");
    assert_eq!(after.scenes["scn_the_breach"].name, "The wall opens");
}

/// Clearing the prose link leaves a scene that is still a scene. A book gets rearranged;
/// a chapter that has not been written yet is a normal state, not a broken record.
#[test]
fn a_scene_with_its_link_cleared_is_still_a_valid_scene() {
    let root = scratch("scene-unlinked");
    let world = open(&root);

    let mut draft = breach_draft(&world);
    draft.prose = Some("   ".into()); // an emptied box, not a link to nowhere
    let plan = plan_scene(&world, draft).expect("plans");
    commit(&plan, plan.revision.as_deref(), false).expect("commits");

    let after = open(&root);
    assert_eq!(after.scenes["scn_the_breach"].prose, None);
    assert!(!fs::read_to_string(root.join("scenes/the-breach.yaml")).unwrap().contains("prose:"));

    let story = wb_story::Story::read(&after);
    let (read, missing) = story.counts();
    assert_eq!((read, missing), (2, 1), "and it reports itself as unread rather than erroring");
}

/// A brand-new scene is written the way the hand-written ones are, in the folder they
/// live in — not as serde's block-style output in whatever directory came to hand.
#[test]
fn a_new_scene_is_written_the_way_the_existing_ones_are() {
    let root = scratch("scene-new");
    let world = open(&root);

    let draft = SceneDraft {
        id: "scn_the_letter".into(),
        name: "The letter".into(),
        date: "0798-05".into(),
        pov: Some("act_maren_vane".into()),
        on_page: vec!["place_corrath_city".into()],
        location: Some("place_corrath_city".into()),
        prose: Some("ch01-the-wall.md#word-from-the-vale".into()),
    };

    let plan = plan_scene(&world, draft).expect("plans");
    commit(&plan, None, false).expect("commits");

    let text = fs::read_to_string(root.join("scenes/the-letter.yaml")).expect("lands in scenes/");
    assert_eq!(
        text,
        "id: scn_the_letter\n\
         name: The letter\n\
         date: \"0798-05\"\n\
         pov: act_maren_vane\n\
         on_page: [place_corrath_city]\n\
         location: place_corrath_city\n\
         prose: ch01-the-wall.md#word-from-the-vale\n",
        "inline lists and a quoted date, like every scene a human wrote"
    );

    assert_eq!(open(&root).scenes.len(), 4);
}

/// The date is what makes a scene checkable, so an unparseable one is refused by name
/// rather than stored as something that will fail to resolve later.
#[test]
fn a_scene_with_a_date_nobody_can_parse_is_refused_before_anything_is_written() {
    let root = scratch("scene-bad-date");
    let world = open(&root);

    let mut draft = breach_draft(&world);
    draft.date = "0812~~".into();
    let err = plan_scene(&world, draft).expect_err("refused");
    assert!(err.contains("`date`"), "got: {err}");
}

/// Moving a scene's date changes what the prose contradicts, and the impact pass says so
/// before the save. This is the whole reason a scene carries a date at all.
#[test]
fn moving_a_scene_off_the_siege_settles_the_contradiction_its_prose_carried() {
    let root = scratch("scene-impact");
    let world = open(&root);

    let mut draft = breach_draft(&world);
    // 0800, not 0810: his death is `0811~`, and the world's year-level fuzz is 730 days,
    // so 0810 is still inside the *doubt* and the finding would rightly survive the move.
    draft.date = "0800".into();

    let plan = plan_scene(&world, draft).expect("plans");
    let resolved: Vec<&str> = plan.impact.resolved.iter().map(|f| f.rule.slug()).collect();

    assert!(
        resolved.contains(&"scene-contradiction"),
        "the preview must show what the move settles, got {resolved:?}"
    );
    assert!(!plan.impact.breaks_something(), "and it breaks nothing");
}
