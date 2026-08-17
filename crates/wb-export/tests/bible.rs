//! What a browser will not tell anybody.
//!
//! An exporter fails in a way no other component does: the output *opens*. A missing
//! anchor renders as a link that does nothing, an unescaped name renders as a slightly
//! odd sentence, and a scope that leaks renders as a perfectly good document containing
//! something the reader was not supposed to see. None of that throws, and none of it
//! shows up in a screenshot — so it is checked structurally, every run.

use std::collections::BTreeSet;
use std::path::PathBuf;

use wb_core::Day;
use wb_export::{Scope, bible};
use wb_store::{World, load};

fn vashen() -> World {
    load(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/vashen"))
        .expect("load the example world")
}

/// Every value of an attribute opening with `prefix`, up to its closing quote.
fn values<'a>(document: &'a str, prefix: &'a str) -> impl Iterator<Item = &'a str> {
    document.split(prefix).skip(1).filter_map(|rest| rest.split('"').next())
}

fn day_of(world: &World, expr: &str) -> Day {
    world.day_of(expr).expect("parses").expect("resolves")
}

#[test]
fn every_link_in_the_document_lands_on_an_anchor_in_the_same_document() {
    let world = vashen();
    for scope in [Scope::Everything, Scope::OnThePage, Scope::AsOf(Day(300_000))] {
        let document = bible(&world, scope);
        let anchors: BTreeSet<&str> = values(&document, "id=\"").collect();
        let dangling: Vec<&str> =
            values(&document, "href=\"#").filter(|t| !anchors.contains(t)).collect();
        assert!(dangling.is_empty(), "{:?} leaves {dangling:?} pointing nowhere", scope.slug());
    }
}

#[test]
fn a_record_whose_name_is_markup_cannot_reach_into_the_page() {
    let world = vashen();
    let mut mischief = world.entities["place_marrow"].clone();
    mischief.name = "Marrow <script>alert(1)</script>".to_string();
    mischief.aliases = vec!["\" onload=\"boom".to_string()];
    let world = world.with_entity(mischief).expect("assembles");

    let document = bible(&world, Scope::Everything);
    assert!(!document.contains("<script>"), "the tag survived into the page");
    assert!(document.contains("&lt;script&gt;alert(1)&lt;/script&gt;"), "and it is shown as text");
    assert!(!document.contains("onload=\"boom"), "and neither did the attribute break out");
}

/// The scope that could only exist in this tool, and the one number that proves it works.
#[test]
fn an_export_as_of_a_date_contains_nothing_that_had_not_happened_yet() {
    let world = vashen();
    let before = bible(&world, Scope::AsOf(day_of(&world, "0810-01-01")));

    assert!(before.contains("9000"), "Marrow still has nine thousand people in it");
    assert!(!before.contains("3100"), "and not the three thousand the siege leaves");
    assert!(!before.contains("The Siege of Marrow"), "which has not happened");
    assert!(before.contains("The Duchy of Corrath"), "and the duchy is still standing");
}

/// A gazetteer written in 810 cannot know when the duke will die, so it does not say.
#[test]
fn an_as_of_export_never_prints_the_end_of_a_window_that_has_not_arrived() {
    let world = vashen();
    let document = bible(&world, Scope::AsOf(day_of(&world, "0810-01-01")));

    let aldric =
        &document[document.find("<article id=\"rec-act_aldric_vane\"").expect("Aldric is in it")..];
    let aldric = &aldric[..aldric.find("</article>").expect("his article ends")];
    assert!(aldric.contains("from 0771-06-12"), "born, and still alive: {aldric}");
    assert!(!aldric.contains("about 0811"), "his death is not in this document");
}

#[test]
fn an_export_of_what_reaches_the_page_leaves_out_what_the_book_never_names() {
    let world = vashen();
    let document = bible(&world, Scope::OnThePage);

    assert!(document.contains("Aldric Vane"), "whom chapter one calls the duke");
    assert!(document.contains("Maren Vane"), "whom it calls Maren");
    assert!(
        !document.contains("The High Tongue"),
        "a language the book never mentions stays below the waterline"
    );
}

/// Precision is the whole point of this world's date grammar, and the export is the one
/// place it could be quietly thrown away by resolving everything to a day number.
#[test]
fn a_date_keeps_the_precision_the_writer_gave_it() {
    let world = vashen();
    let document = bible(&world, Scope::Everything);

    assert!(document.contains("about 0602"), "Marrow was founded `0602~`");
    assert!(document.contains("about 0812-04"), "and the siege is dated only to the month");
    assert!(
        !document.contains("0602-01-01"),
        "a year-precision date must never be printed as a day"
    );
    assert!(!document.contains("0812-04-01"), "nor a month-precision one");
}

/// `@evt_oath_of_vashen` is bookkeeping. A reader gets the year it worked out to.
#[test]
fn an_anchor_is_translated_rather_than_shown_to_the_reader() {
    let world = vashen();
    let document = bible(&world, Scope::Everything);
    for prefix in ["@evt_", "@act_", "@place_", "@pol_", "@ter_", "@scn_", "@thing_"] {
        assert!(!document.contains(prefix), "`{prefix}` is bookkeeping, not something to read");
    }
    assert!(document.contains("0806-02-14"), "the oath's own date stands in for it");
}

#[test]
fn a_world_with_no_map_still_exports() {
    let mut world = vashen();
    world.map = None;
    let document = bible(&world, Scope::Everything);
    assert!(document.contains("The Vashen Reckoning"));
    assert!(!document.contains("<image"), "and simply has no map figure in it");
}

#[test]
fn the_document_is_self_contained() {
    let world = vashen();
    let document = bible(&world, Scope::Everything);

    // The SVG namespace is a URI and not a fetch, so it is the one `http://` allowed.
    let fetched = document.replace("xmlns=\"http://www.w3.org/2000/svg\"", "");
    for outside in ["http://", "https://", "src=\"", "<link", "<script", "@import"] {
        assert!(!fetched.contains(outside), "`{outside}` would make this need a network");
    }
    assert!(document.starts_with("<!doctype html>"), "and it is a whole document");
    assert!(document.contains("data:image/png;base64,"), "with the map inside it");
}
