//! The review loop, end to end: load, measure impact, apply to disk, record decision.

use std::fs;
use std::path::{Path, PathBuf};

use wb_check::Certainty;
use wb_propose::{Change, Proposal, Status, accept, impact, preview, reject, store};
use wb_store::{World, load};

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/vashen")
}

fn vashen() -> World {
    load(example_root()).expect("load example world")
}

fn proposals() -> Vec<Proposal> {
    store::load_all(example_root()).expect("load proposals")
}

fn by_id(id: &str) -> Proposal {
    proposals().into_iter().find(|p| p.id == id).unwrap_or_else(|| panic!("no proposal {id}"))
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

/// A throwaway copy of the example world, so disk tests never touch the real one.
fn scratch_world(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wb-propose-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    copy_dir(&example_root(), &dir);
    dir
}

// ------------------------------------------------------------ loading

#[test]
fn the_example_world_ships_two_pending_proposals() {
    let all = proposals();
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|p| p.is_pending()));
    assert!(all.iter().all(|p| !p.source.as_os_str().is_empty()));
}

#[test]
fn a_world_with_no_proposals_folder_is_not_an_error() {
    assert!(
        store::load_all(std::env::temp_dir().join("definitely-not-a-world")).unwrap().is_empty()
    );
}

// ------------------------------------------------------------ impact

/// The point of the queue: see what a change settles before accepting it.
#[test]
fn a_good_proposal_resolves_the_open_question_and_breaks_nothing() {
    let world = vashen();
    let effect = impact(&world, &by_id("prp_resolve_aldric")).unwrap();

    assert_eq!(effect.before, (0, 1), "the world starts with one open question");
    assert_eq!(effect.after, (0, 0), "and the proposal settles it");
    assert_eq!(effect.resolved.len(), 1);
    assert!(effect.resolved[0].message.contains("Aldric Vane"));
    assert!(effect.introduced.is_empty(), "{:#?}", effect.introduced);
    assert!(!effect.breaks_something());
}

#[test]
fn a_plausible_but_wrong_proposal_is_caught_before_it_lands() {
    let world = vashen();
    let effect = impact(&world, &by_id("prp_vashen_ruled_from_marrow")).unwrap();

    assert!(effect.breaks_something());
    assert_eq!(effect.after.0, 2, "two definite contradictions: {:#?}", effect.introduced);
    assert!(effect.resolved.is_empty(), "it settles nothing");

    let rules: Vec<&str> = effect.introduced.iter().map(|f| f.rule.slug()).collect();
    assert!(rules.contains(&"anachronistic-fact"), "{rules:?}");
    assert!(rules.contains(&"conflicting-facts"), "{rules:?}");
    assert!(effect.introduced.iter().all(|f| f.certainty == Certainty::Definite));
}

#[test]
fn simulating_never_touches_the_world_it_was_given() {
    let world = vashen();
    let before = wb_check::check(&world).counts();
    let _ = impact(&world, &by_id("prp_resolve_aldric")).unwrap();
    assert_eq!(wb_check::check(&world).counts(), before);
}

// ------------------------------------------------------------ preview

#[test]
fn preview_patches_the_frontmatter_and_keeps_the_prose() {
    let world = vashen();
    let edits = preview(&world, &by_id("prp_resolve_aldric")).unwrap();

    assert_eq!(edits.len(), 1);
    let edit = &edits[0];
    assert!(edit.path.ends_with("aldric-vane.md"));
    assert!(!edit.is_new());
    assert!(edit.changes_anything());
    assert!(!edit.reformats(), "the writer's own formatting is kept");

    assert!(edit.after.contains("@evt_siege_of_marrow+1y"), "the new date is written");
    assert!(edit.after.contains("Fourth of his name"), "the prose body survives");
    assert!(edit.after.starts_with("---\n"), "still a frontmatter document");

    // And what it produces must load again as the same record.
    let doc = wb_store::frontmatter::split(&edit.after).expect("frontmatter");
    let reparsed: wb_store::Entity = serde_yaml_bw::from_str(doc.frontmatter).expect("reparse");
    assert_eq!(reparsed.id, "act_aldric_vane");
    assert_eq!(reparsed.facts.len(), 5);
}

#[test]
fn creating_a_record_picks_a_folder_from_its_primitive() {
    let world = vashen();
    let proposal = Proposal {
        id: "prp_new".into(),
        title: "Add a city".into(),
        author: String::new(),
        note: String::new(),
        status: Status::Pending,
        source: PathBuf::new(),
        changes: vec![Change::CreateEntity {
            id: "place_greyford".into(),
            name: "Greyford".into(),
            type_name: "city".into(),
            existence: None,
            parents: Vec::new(),
            facts: Vec::new(),
        }],
    };

    let edits = preview(&world, &proposal).unwrap();
    assert_eq!(edits.len(), 1);
    assert!(edits[0].is_new());
    assert!(
        edits[0].path.ends_with("entities/places/greyford.md"),
        "got {}",
        edits[0].path.display()
    );
}

// ------------------------------------------------------------ applying

#[test]
fn accepting_writes_the_files_and_records_the_decision() {
    let root = scratch_world("accept");
    let world = load(&root).unwrap();
    let mut proposal =
        store::load_all(&root).unwrap().into_iter().find(|p| p.id == "prp_resolve_aldric").unwrap();

    let written = accept(&world, &mut proposal).unwrap();
    assert_eq!(written.len(), 1);
    assert_eq!(proposal.status, Status::Accepted);

    // The world on disk now says what the proposal asked for...
    let reloaded = load(&root).unwrap();
    assert!(wb_check::check(&reloaded).is_clean(), "{:#?}", wb_check::check(&reloaded).findings);
    assert!(reloaded.entities["act_aldric_vane"].body.contains("Fourth of his name"));

    // ...and the decision is on disk too, not just in memory.
    let after = store::load_all(&root).unwrap();
    let recorded = after.iter().find(|p| p.id == "prp_resolve_aldric").unwrap();
    assert_eq!(recorded.status, Status::Accepted);
    assert_eq!(recorded.changes.len(), 1, "the record of what was asked for is kept");
}

#[test]
fn a_decided_proposal_cannot_be_decided_again() {
    let root = scratch_world("twice");
    let world = load(&root).unwrap();
    let mut proposal =
        store::load_all(&root).unwrap().into_iter().find(|p| p.id == "prp_resolve_aldric").unwrap();

    accept(&world, &mut proposal).unwrap();
    let err = accept(&world, &mut proposal).unwrap_err();
    assert!(matches!(err, wb_propose::Error::NotPending { .. }), "got {err}");
}

#[test]
fn rejecting_records_the_decision_without_touching_the_world() {
    let root = scratch_world("reject");
    let before = fs::read_to_string(root.join("entities/actors/aldric-vane.md")).unwrap();

    let mut proposal =
        store::load_all(&root).unwrap().into_iter().find(|p| p.id == "prp_resolve_aldric").unwrap();
    reject(&mut proposal).unwrap();

    assert_eq!(proposal.status, Status::Rejected);
    assert_eq!(fs::read_to_string(root.join("entities/actors/aldric-vane.md")).unwrap(), before);
    let after = store::load_all(&root).unwrap();
    assert_eq!(
        after.iter().find(|p| p.id == "prp_resolve_aldric").unwrap().status,
        Status::Rejected
    );
}

/// The promise that exists because this tool holds people's life's work.
///
/// This used to be a refusal: a key the model did not understand blocked the write
/// outright, because rewriting the frontmatter canonically would have dropped it. Now
/// the applier patches in place, so the key is simply carried through — which is the
/// same promise kept better. `WouldDropKey` still stands behind the canonical fallback,
/// for the files this writer will not risk patching.
#[test]
fn a_key_the_model_does_not_understand_is_carried_through_untouched() {
    let root = scratch_world("unknown-key");
    let path = root.join("entities/actors/aldric-vane.md");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replacen("---\n", "---\nprivate_notes: do not lose me\n", 1)).unwrap();

    let world = load(&root).unwrap();
    let mut proposal =
        store::load_all(&root).unwrap().into_iter().find(|p| p.id == "prp_resolve_aldric").unwrap();

    let edits = preview(&world, &proposal).expect("the unknown key no longer blocks the write");
    let edit = edits.iter().find(|e| e.path == path).expect("aldric's file is edited");
    assert!(edit.after.contains("private_notes: do not lose me"), "got:\n{}", edit.after);
    assert!(!edit.reformats(), "the file should have been patched, not rewritten");

    accept(&world, &mut proposal).expect("accepts");
    let written = fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("private_notes: do not lose me"),
        "the key was lost on the way to disk"
    );
}

/// A one-field change now reads as a one-field diff, which is the review queue's whole
/// selling point: an impact analysis nobody can find inside a reformat is not much use.
#[test]
fn accepting_a_proposal_changes_only_the_lines_it_means_to() {
    let root = scratch_world("narrow-diff");
    let path = root.join("entities/actors/aldric-vane.md");
    let before = fs::read_to_string(&path).unwrap();

    let world = load(&root).unwrap();
    let mut proposal =
        store::load_all(&root).unwrap().into_iter().find(|p| p.id == "prp_resolve_aldric").unwrap();
    accept(&world, &mut proposal).expect("accepts");

    let after = fs::read_to_string(&path).unwrap();
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    assert_eq!(old.len(), new.len(), "the line count moved:\n{after}");
    let changed = (0..old.len()).filter(|&i| old[i] != new[i]).count();
    assert_eq!(changed, 1, "expected one changed line, got {changed}:\n{after}");
}

/// The comments in a file the queue touches are the writer's, not the queue's.
#[test]
fn accepting_a_proposal_keeps_the_comments_in_the_file_it_edits() {
    let root = scratch_world("keeps-comments");
    let path = root.join("entities/actors/aldric-vane.md");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replacen("---\n", "---\n# the parish register is water-damaged\n", 1))
        .unwrap();

    let world = load(&root).unwrap();
    let mut proposal =
        store::load_all(&root).unwrap().into_iter().find(|p| p.id == "prp_resolve_aldric").unwrap();
    accept(&world, &mut proposal).expect("accepts");

    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("# the parish register is water-damaged"), "got:\n{after}");
}

// ------------------------------------------------------------ rejected changes

fn one_change(change: Change) -> Proposal {
    Proposal {
        id: "prp_test".into(),
        title: "test".into(),
        author: String::new(),
        note: String::new(),
        status: Status::Pending,
        changes: vec![change],
        source: PathBuf::new(),
    }
}

#[test]
fn changes_to_records_that_do_not_exist_are_refused() {
    let world = vashen();
    let proposal = one_change(Change::SetEventDate {
        event: "evt_imaginary".into(),
        date: wb_core::parse_date("0900").unwrap(),
    });
    assert!(matches!(
        wb_propose::simulate(&world, &proposal).unwrap_err(),
        wb_propose::Error::UnknownTarget { .. }
    ));
}

#[test]
fn creating_something_that_already_exists_is_refused() {
    let world = vashen();
    let proposal = one_change(Change::CreateEntity {
        id: "act_aldric_vane".into(),
        name: "Aldric Again".into(),
        type_name: "noble".into(),
        existence: None,
        parents: Vec::new(),
        facts: Vec::new(),
    });
    assert!(matches!(
        wb_propose::simulate(&world, &proposal).unwrap_err(),
        wb_propose::Error::AlreadyExists { .. }
    ));
}

#[test]
fn removing_a_fact_that_is_not_there_is_refused() {
    let world = vashen();
    let proposal = one_change(Change::RemoveFact {
        entity: "act_aldric_vane".into(),
        attr: "title".into(),
        value: wb_store::Value::Text("King of Everywhere".into()),
    });
    assert!(matches!(
        wb_propose::simulate(&world, &proposal).unwrap_err(),
        wb_propose::Error::NoSuchFact { .. }
    ));
}

#[test]
fn removing_a_fact_that_is_there_works() {
    let world = vashen();
    let proposal = one_change(Change::RemoveFact {
        entity: "act_aldric_vane".into(),
        attr: "title".into(),
        value: wb_store::Value::Text("Duke of Corrath".into()),
    });
    let after = wb_propose::simulate(&world, &proposal).unwrap();
    assert_eq!(after.entities["act_aldric_vane"].facts.len(), 4);
}
