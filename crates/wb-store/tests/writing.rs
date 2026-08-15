//! Rewriting records without disturbing the file around them.
//!
//! The fixtures here are mostly the shipped example world's actual bytes rather than
//! hand-made strings, because the failure this code exists to prevent is not "the
//! algorithm is wrong in the abstract" — it is "somebody's real file came back different".
//! A synthetic fixture cannot fail that way; `examples/vashen` can, and it carries the
//! two comment placements that motivated the whole thing.

use std::path::{Path, PathBuf};

use wb_store::write::{Fidelity, render_entity, render_event};
use wb_store::{Entity, Value, World, load};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/vashen")
}

fn vashen() -> World {
    let r = root();
    load(&r).unwrap_or_else(|e| panic!("loading {}: {e}", r.display()))
}

fn text_of(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

/// Render `entity` over its own file and return the bytes.
fn rewrite(entity: &Entity) -> (String, Fidelity) {
    let original = text_of(&entity.source);
    let out = render_entity(&entity.source, Some(&original), entity).expect("renders");
    (out.text, out.fidelity)
}

fn marrow(world: &World) -> Entity {
    world.entities["place_marrow"].clone()
}

// ---- the guarantee

/// The whole promise, as one assertion: writing a record back unchanged changes nothing.
///
/// If this fails, nothing else in this file matters — every other test is about *which*
/// bytes survive an edit, and this is the one that says the baseline is zero.
#[test]
fn every_record_in_the_example_world_survives_a_no_op_save_byte_for_byte() {
    let world = vashen();

    for entity in world.entities.values() {
        let original = text_of(&entity.source);
        let out = render_entity(&entity.source, Some(&original), entity).expect("renders");
        assert_eq!(
            out.fidelity,
            Fidelity::Preserved,
            "{} fell back to a canonical rewrite",
            entity.source.display()
        );
        assert_eq!(out.text, original, "{} was not left alone", entity.source.display());
    }

    for event in world.events.values() {
        let original = text_of(&event.source);
        let out = render_event(&event.source, Some(&original), event).expect("renders");
        assert_eq!(
            out.fidelity,
            Fidelity::Preserved,
            "{} fell back to a canonical rewrite",
            event.source.display()
        );
        assert_eq!(out.text, original, "{} was not left alone", event.source.display());
    }
}

/// A sweep, so a field nobody thought to test is still covered. Every record, every
/// field the model has, mutated one at a time and read back.
#[test]
fn patching_any_single_field_of_any_example_record_round_trips() {
    let world = vashen();

    for entity in world.entities.values() {
        let original = text_of(&entity.source);
        let mut variants: Vec<(&str, Entity)> = Vec::new();

        let mut renamed = entity.clone();
        renamed.name = "Renamed".into();
        variants.push(("name", renamed));

        let mut retyped = entity.clone();
        retyped.type_name = "hamlet".into();
        variants.push(("type", retyped));

        let mut moved = entity.clone();
        moved.marker = Some([0.125, 0.875]);
        variants.push(("marker", moved));

        let mut cleared = entity.clone();
        cleared.marker = None;
        variants.push(("marker cleared", cleared));

        let mut reshaped = entity.clone();
        reshaped.shape = vec![[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]];
        variants.push(("shape", reshaped));

        let mut factless = entity.clone();
        factless.facts.clear();
        variants.push(("facts cleared", factless));

        let mut extra = entity.clone();
        extra.facts.push(wb_store::Fact {
            attr: "note".into(),
            value: Value::Text("added".into()),
            from: wb_core::DateExpr::Unknown,
            to: wb_core::DateExpr::Unknown,
        });
        variants.push(("fact appended", extra));

        for (what, want) in variants {
            let out = render_entity(&entity.source, Some(&original), &want).expect("renders");
            let region = wb_store::frontmatter::split_spans(&out.text)
                .map(|s| s.frontmatter)
                .unwrap_or(0..out.text.len());
            let mut back: Entity = serde_yaml_bw::from_str(&out.text[region.start..region.end])
                .unwrap_or_else(|e| {
                    panic!("{} after changing {what}: {e}\n{}", entity.id, out.text)
                });
            back.body = want.body.clone();
            back.source = want.source.clone();
            assert_eq!(back, want, "{} did not survive changing {what}", entity.id);
        }
    }
}

// ---- the comments that motivated this

/// `corrath.md` explains one of its facts with a comment inside the `facts:` list. Edit
/// the fact next to it and that explanation has to still be there.
#[test]
fn a_comment_inside_facts_survives_an_edit_to_its_neighbour() {
    let world = vashen();
    let corrath = world.entities["pol_corrath"].clone();
    let original = text_of(&corrath.source);
    let comment = original
        .lines()
        .find(|l| l.trim_start().starts_with('#'))
        .expect("corrath.md carries a comment in its facts")
        .to_string();

    let mut edited = corrath.clone();
    let first = edited.facts.first_mut().expect("has facts");
    first.value = Value::Text("changed".into());

    let out = render_entity(&corrath.source, Some(&original), &edited).expect("renders");
    assert_eq!(out.fidelity, Fidelity::Preserved);
    assert!(out.text.contains(comment.trim()), "the comment was lost:\n{}", out.text);
    assert!(out.text.contains("changed"), "the edit did not land");
}

/// The siege event is half comments, including three lines explaining why Aldric is
/// listed. Re-dating the event must not take the explanation with it.
#[test]
fn the_comment_block_above_participants_stays_above_participants() {
    let world = vashen();
    let siege = world.events["evt_siege_of_marrow"].clone();
    let original = text_of(&siege.source);

    let mut edited = siege.clone();
    edited.date = wb_core::parse_date("0813-05").expect("parses");

    let out = render_event(&siege.source, Some(&original), &edited).expect("renders");
    assert_eq!(out.fidelity, Fidelity::Preserved);
    for line in original.lines().filter(|l| l.trim_start().starts_with('#')) {
        assert!(out.text.contains(line.trim()), "lost the comment {line:?}");
    }
    assert!(out.text.contains("0813-05"), "the new date did not land");
}

/// The measurable claim behind the review queue: a one-field edit is a one-line diff.
#[test]
fn changing_one_fact_leaves_every_other_line_untouched() {
    let world = vashen();
    let m = marrow(&world);
    let original = text_of(&m.source);

    let mut edited = m.clone();
    edited.facts[0].value = Value::Int(9500);

    let (text, fidelity) = {
        let out = render_entity(&m.source, Some(&original), &edited).expect("renders");
        (out.text, out.fidelity)
    };
    assert_eq!(fidelity, Fidelity::Preserved);

    let before: Vec<&str> = original.lines().collect();
    let after: Vec<&str> = text.lines().collect();
    assert_eq!(before.len(), after.len(), "the line count moved");
    let changed: Vec<usize> = (0..before.len()).filter(|&i| before[i] != after[i]).collect();
    assert_eq!(changed.len(), 1, "expected exactly one changed line, got {changed:?}");
    assert!(after[changed[0]].contains("9500"));
}

#[test]
fn an_inline_flow_mapping_stays_inline() {
    let world = vashen();
    let m = marrow(&world);
    let original = text_of(&m.source);
    assert!(original.contains("existence: {"), "the fixture must use flow style");

    let mut edited = m.clone();
    edited.existence = Some(wb_store::Span {
        from: wb_core::parse_date("0601~").expect("parses"),
        to: wb_core::DateExpr::Unknown,
    });

    let out = render_entity(&m.source, Some(&original), &edited).expect("renders");
    assert!(out.text.contains("existence: { from: \"0601~\" }"), "got:\n{}", out.text);
}

#[test]
fn the_prose_body_is_never_touched() {
    let world = vashen();
    let m = marrow(&world);
    let original = text_of(&m.source);
    let body = original.split("---").nth(2).expect("has a body").to_string();

    let mut edited = m.clone();
    edited.name = "Marrow-on-the-Wall".into();

    let out = render_entity(&m.source, Some(&original), &edited).expect("renders");
    assert!(out.text.ends_with(&body), "the prose below the fence changed");
}

// ---- files this writer refuses to patch

fn patch_str(name: &str, original: &str, edit: impl FnOnce(&mut Entity)) -> (String, Fidelity) {
    let path = PathBuf::from(name);
    let region = wb_store::frontmatter::split_spans(original)
        .map(|s| s.frontmatter)
        .unwrap_or(0..original.len());
    let mut entity: Entity =
        serde_yaml_bw::from_str(&original[region.start..region.end]).expect("fixture parses");
    entity.source = path.clone();
    edit(&mut entity);
    let out = render_entity(&path, Some(original), &entity).expect("renders");
    (out.text, out.fidelity)
}

#[test]
fn an_alias_in_frontmatter_refuses_to_patch_rather_than_guessing() {
    let original = "---\nid: place_x\nname: &n Marrow\ntype: city\nalt: *n\n---\nbody\n";
    let (_, fidelity) = patch_str("x.md", original, |e| e.type_name = "town".into());
    match fidelity {
        Fidelity::Reformatted { reason, .. } => {
            assert!(reason.contains("anchor") || reason.contains("alias"), "reason was {reason:?}")
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// Tabs are illegal as YAML indentation, so a file carrying them is one this writer has
/// no business reasoning about. It has to come back as a refusal, not as a guess.
#[test]
fn tabs_in_the_indentation_bail_out_instead_of_guessing() {
    let original =
        "---\nid: place_x\nname: Marrow\ntype: city\nexistence:\n\tfrom: \"0602\"\n---\nb\n";
    let want = Entity {
        id: "place_x".into(),
        name: "Other".into(),
        type_name: "city".into(),
        existence: None,
        parents: Vec::new(),
        facts: Vec::new(),
        marker: None,
        shape: Vec::new(),
        body: "b".into(),
        source: PathBuf::from("x.md"),
    };
    let out = render_entity(Path::new("x.md"), Some(original), &want).expect("renders");
    assert!(
        matches!(out.fidelity, Fidelity::Reformatted { .. }),
        "tab indentation should not be patched, got {:?}",
        out.fidelity
    );
}

/// The behaviour that replaces `WouldDropKey` on the patch path: a key this version has
/// never heard of is carried through untouched rather than blocking the write.
#[test]
fn a_key_the_model_does_not_understand_is_carried_through_untouched() {
    let original =
        "---\nid: place_x\nname: Marrow\ntype: city\nmood: ominous\ncolour: grey\n---\nbody\n";
    let (text, fidelity) = patch_str("x.md", original, |e| e.name = "Marrow-on-the-Wall".into());
    assert_eq!(fidelity, Fidelity::Preserved);
    assert!(text.contains("mood: ominous"), "an unmodelled key was dropped:\n{text}");
    assert!(text.contains("colour: grey"), "an unmodelled key was dropped:\n{text}");
    assert!(text.contains("name: Marrow-on-the-Wall"));
}

#[test]
fn windows_line_endings_stay_windows_line_endings() {
    let original = "---\r\nid: place_x\r\nname: Marrow\r\ntype: city\r\n---\r\nbody\r\n";
    let (text, fidelity) = patch_str("x.md", original, |e| e.name = "Other".into());
    assert_eq!(fidelity, Fidelity::Preserved);
    assert!(text.contains("name: Other\r\n"), "a line ending changed:\n{text:?}");
    assert!(!text.contains("\n\n"), "no bare newline should have crept in");
}

#[test]
fn a_byte_order_mark_is_still_there_afterwards() {
    let original = "\u{feff}---\nid: place_x\nname: Marrow\ntype: city\n---\nbody\n";
    let (text, fidelity) = patch_str("x.md", original, |e| e.name = "Other".into());
    assert_eq!(fidelity, Fidelity::Preserved);
    assert!(text.starts_with('\u{feff}'), "the BOM was dropped");
}

#[test]
fn a_new_file_is_rendered_canonically_and_says_so() {
    let entity = Entity {
        id: "place_new".into(),
        name: "New".into(),
        type_name: "city".into(),
        existence: None,
        parents: Vec::new(),
        facts: Vec::new(),
        marker: Some([0.5, 0.5]),
        shape: Vec::new(),
        body: "Somewhere new.".into(),
        source: PathBuf::new(),
    };
    let out = render_entity(Path::new("new.md"), None, &entity).expect("renders");
    assert_eq!(out.fidelity, Fidelity::Created);
    assert!(out.text.starts_with("---\n"));
    assert!(out.text.contains("Somewhere new."));
}

// ---- adding and removing

#[test]
fn adding_a_fact_appends_it_after_the_last_one() {
    let world = vashen();
    let m = marrow(&world);
    let original = text_of(&m.source);

    let mut edited = m.clone();
    edited.facts.push(wb_store::Fact {
        attr: "walls".into(),
        value: Value::Text("granite".into()),
        from: wb_core::DateExpr::Unknown,
        to: wb_core::DateExpr::Unknown,
    });

    let (text, fidelity) = rewrite(&m);
    assert_eq!(text, original, "control: an untouched record is untouched");
    assert_eq!(fidelity, Fidelity::Preserved);

    let out = render_entity(&m.source, Some(&original), &edited).expect("renders");
    assert_eq!(out.fidelity, Fidelity::Preserved);
    let added = out.text.find("walls").expect("the fact was added");
    let last_old = out.text.rfind("population").expect("the old facts are still there");
    assert!(added > last_old, "the new fact should come after the existing ones");
}

#[test]
fn removing_a_fact_removes_only_its_lines() {
    let world = vashen();
    let m = marrow(&world);
    let original = text_of(&m.source);

    let mut edited = m.clone();
    let dropped = edited.facts.remove(0);

    let out = render_entity(&m.source, Some(&original), &edited).expect("renders");
    assert_eq!(out.fidelity, Fidelity::Preserved);

    let before = original.lines().count();
    let after = out.text.lines().count();
    assert!(after < before, "lines should have gone away");
    assert!(
        before - after <= 4,
        "removing one fact should not rewrite the file: {before} -> {after}"
    );
    assert!(out.text.contains("attr: population"), "the second population fact remains");
    let _ = dropped;
}

#[test]
fn a_marker_can_be_added_to_a_record_that_had_none() {
    let world = vashen();
    let lang = world.entities["thing_high_tongue"].clone();
    let original = text_of(&lang.source);
    assert!(!original.contains("marker:"), "the fixture must start without one");

    let mut edited = lang.clone();
    edited.marker = Some([0.25, 0.75]);

    let out = render_entity(&lang.source, Some(&original), &edited).expect("renders");
    assert_eq!(out.fidelity, Fidelity::Preserved);
    assert!(out.text.contains("marker: [0.25, 0.75]"), "got:\n{}", out.text);
}
