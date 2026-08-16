//! Two halves: the example world should be *almost* clean, and each definite rule
//! gets a purpose-built broken world to catch it.

use std::path::PathBuf;

use wb_check::{Certainty, Rule, check};
use wb_core::{Calendar, Fuzz, Month, parse_date};
use wb_store::{Entity, Event, Fact, Rules, Span, Value, World, WorldDef, load};

// ------------------------------------------------------------ the example world

fn vashen() -> World {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/vashen");
    load(&root).unwrap_or_else(|e| panic!("loading {}: {e}", root.display()))
}

/// The seed world is deliberately not clean: the Marrow chronicle puts Aldric at the
/// walls a year after his recorded death. That is the one thing the engine should say,
/// and it should say "possible" rather than picking a side.
#[test]
fn the_example_world_has_exactly_one_open_question() {
    let report = check(&vashen());
    let (definite, possible) = report.counts();

    assert_eq!(
        definite,
        0,
        "the seed world should have no outright errors, got: {:#?}",
        report.definite().map(|f| &f.message).collect::<Vec<_>>()
    );
    assert_eq!(
        possible,
        1,
        "expected exactly the Aldric question, got: {:#?}",
        report.possible().map(|f| &f.message).collect::<Vec<_>>()
    );

    let finding = &report.findings[0];
    assert_eq!(finding.rule, Rule::ExistenceViolation);
    assert_eq!(finding.certainty, Certainty::Possible);
    assert_eq!(finding.subject, "evt_siege_of_marrow");
    assert!(finding.related.contains(&"act_aldric_vane".to_string()));
    assert!(finding.message.contains("Aldric Vane"), "{}", finding.message);
    assert!(finding.at.is_some(), "a finding with a date should be jumpable to");
    assert_eq!(finding.sources.len(), 2, "both files are worth opening");
}

/// The duchy is annexed *by* the siege and takes part in it; the city is founded *by*
/// the founding and hosts it. Both look like boundary violations and neither is one.
#[test]
fn entities_bounded_by_an_event_are_not_flagged_against_it() {
    let report = check(&vashen());
    for finding in &report.findings {
        assert!(
            !finding.related.contains(&"pol_corrath".to_string()),
            "the duchy's own annexation should not be a finding: {}",
            finding.message
        );
    }
}

/// The contested Vale has two owners whose vague windows overlap. That is the feature.
#[test]
fn a_vague_handover_is_never_reported_as_a_conflict() {
    let report = check(&vashen());
    assert_eq!(report.of_rule(Rule::ConflictingFacts).count(), 0);
    assert_eq!(report.of_rule(Rule::SuccessionGap).count(), 0);
}

// ------------------------------------------------------------ synthetic worlds

fn calendar() -> Calendar {
    Calendar::new("Test", (1..=12).map(|i| Month::new(format!("M{i}"), 30)).collect()).unwrap()
}

fn def() -> WorldDef {
    WorldDef {
        name: "Test".into(),
        calendar: calendar(),
        fuzz: Fuzz::default(),
        map: None,
        manuscript: None,
        types: Vec::new(),
        rules: Rules::default(),
    }
}

fn ent(id: &str) -> Entity {
    Entity {
        id: id.into(),
        name: id.into(),
        aliases: Vec::new(),
        type_name: "thing".into(),
        existence: None,
        parents: Vec::new(),
        facts: Vec::new(),
        marker: None,
        shape: Vec::new(),
        body: String::new(),
        source: PathBuf::from(format!("{id}.md")),
    }
}

fn lived(mut e: Entity, from: &str, to: &str) -> Entity {
    e.existence = Some(Span { from: parse_date(from).unwrap(), to: parse_date(to).unwrap() });
    e
}

fn with_fact(mut e: Entity, attr: &str, value: &str, from: &str, to: &str) -> Entity {
    e.facts.push(Fact {
        attr: attr.into(),
        value: Value::Text(value.into()),
        from: parse_date(from).unwrap(),
        to: parse_date(to).unwrap(),
    });
    e
}

fn evt(id: &str, date: &str, participants: &[&str]) -> Event {
    Event {
        id: id.into(),
        name: id.into(),
        kind: String::new(),
        date: parse_date(date).unwrap(),
        participants: participants.iter().map(|s| s.to_string()).collect(),
        location: None,
        body: String::new(),
        source: PathBuf::from(format!("{id}.yaml")),
    }
}

fn world(entities: Vec<Entity>, events: Vec<Event>) -> World {
    World::assemble(PathBuf::from("."), def(), entities, events, Vec::new()).expect("assemble")
}

fn only(world: &World, rule: Rule) -> (Certainty, String) {
    let report = check(world);
    let found: Vec<_> = report.of_rule(rule).collect();
    assert_eq!(found.len(), 1, "expected one {rule:?}, got {:#?}", report.findings);
    (found[0].certainty, found[0].message.clone())
}

#[test]
fn a_participant_who_was_certainly_dead_is_definite() {
    let w = world(
        vec![lived(ent("act_ghost"), "0700-01-01", "0750-01-01")],
        vec![evt("evt_council", "0800-01-01", &["act_ghost"])],
    );
    let (certainty, message) = only(&w, Rule::ExistenceViolation);
    assert_eq!(certainty, Certainty::Definite);
    assert!(message.contains("existed only"), "{message}");
}

#[test]
fn the_same_case_with_a_vague_death_is_only_possible() {
    // "died around 0750" leaves room; the engine must not decide for the writer.
    let w = world(
        vec![lived(ent("act_ghost"), "0700-01-01", "0750~")],
        vec![evt("evt_council", "0751-06-01", &["act_ghost"])],
    );
    let (certainty, message) = only(&w, Rule::ExistenceViolation);
    assert_eq!(certainty, Certainty::Possible);
    assert!(message.contains("may fall outside"), "{message}");
}

#[test]
fn a_participant_alive_throughout_produces_nothing() {
    let w = world(
        vec![lived(ent("act_alive"), "0700-01-01", "0900-01-01")],
        vec![evt("evt_council", "0800-01-01", &["act_alive"])],
    );
    assert!(check(&w).is_clean(), "{:#?}", check(&w).findings);
}

#[test]
fn one_attribute_asserted_two_ways_at_once_is_a_conflict() {
    let vale = with_fact(
        with_fact(ent("ter_vale"), "owner", "pol_a", "0500-01-01", "0800-01-01"),
        "owner",
        "pol_b",
        "0700-01-01",
        "0900-01-01",
    );
    let w = world(vec![vale, ent("pol_a"), ent("pol_b")], vec![]);
    let (certainty, message) = only(&w, Rule::ConflictingFacts);
    assert_eq!(certainty, Certainty::Definite);
    assert!(message.contains("both pol_a and pol_b"), "{message}");
}

#[test]
fn attributes_declared_multi_valued_are_exempt() {
    let guild = with_fact(
        with_fact(ent("pol_guild"), "member", "act_a", "0500-01-01", "0800-01-01"),
        "member",
        "act_b",
        "0700-01-01",
        "0900-01-01",
    );
    let mut definition = def();
    definition.rules.multi_valued = vec!["member".into()];
    let w = World::assemble(
        PathBuf::from("."),
        definition,
        vec![guild, ent("act_a"), ent("act_b")],
        vec![],
        Vec::new(),
    )
    .unwrap();
    assert_eq!(check(&w).of_rule(Rule::ConflictingFacts).count(), 0);
}

#[test]
fn references_to_nothing_are_found_wherever_they_hide() {
    let mut child = ent("act_child");
    child.parents = vec!["act_missing".into()];
    let holder = with_fact(ent("ter_land"), "owner", "pol_missing", "0500-01-01", "?");
    let mut event = evt("evt_thing", "0600-01-01", &["act_nobody"]);
    event.location = Some("place_nowhere".into());

    let w = world(vec![child, holder, ent("pol_real")], vec![event]);
    let report = check(&w);
    let orphans: Vec<&String> =
        report.of_rule(Rule::OrphanReference).map(|f| &f.related[0]).collect();

    assert_eq!(orphans.len(), 4, "{:#?}", report.findings);
    for missing in ["act_missing", "pol_missing", "act_nobody", "place_nowhere"] {
        assert!(orphans.iter().any(|o| *o == missing), "{missing} not reported");
    }
}

/// The prefix heuristic is self-calibrating: `pol_` is an id prefix in this world,
/// `iron_` is not, so a value like `iron_ore` is a value and not a broken link.
#[test]
fn a_value_that_merely_looks_like_an_id_is_left_alone() {
    let mine = with_fact(ent("place_mine"), "yields", "iron_ore", "?", "?");
    let w = world(vec![mine, ent("pol_real")], vec![]);
    assert_eq!(check(&w).of_rule(Rule::OrphanReference).count(), 0);
}

#[test]
fn a_stretch_nothing_covers_is_a_succession_gap() {
    let duchy = with_fact(
        with_fact(ent("pol_duchy"), "ruler", "act_a", "0500-01-01", "0600-01-01"),
        "ruler",
        "act_b",
        "0700-01-01",
        "0800-01-01",
    );
    let w = world(vec![duchy, ent("act_a"), ent("act_b")], vec![]);
    let (certainty, message) = only(&w, Rule::SuccessionGap);
    assert_eq!(certainty, Certainty::Definite);
    assert!(message.contains("no ruler"), "{message}");
}

/// Two facts meeting at a vague event leave a hole between their *certain* cores.
/// That hole is uncertainty, not an unruled century, and must not be reported.
#[test]
fn facts_meeting_at_a_vague_event_leave_no_gap() {
    let duchy = with_fact(
        with_fact(ent("pol_duchy"), "ruler", "act_a", "0500-01-01", "@evt_coup"),
        "ruler",
        "act_b",
        "@evt_coup",
        "0800-01-01",
    );
    let w = world(vec![duchy, ent("act_a"), ent("act_b")], vec![evt("evt_coup", "0600~", &[])]);
    let report = check(&w);
    assert_eq!(report.of_rule(Rule::SuccessionGap).count(), 0, "{:#?}", report.findings);
    assert_eq!(report.of_rule(Rule::ConflictingFacts).count(), 0);
}

#[test]
fn a_child_cannot_predate_their_parent() {
    let mut child = ent("act_child");
    child.parents = vec!["act_parent".into()];
    let w = world(
        vec![
            lived(child, "0700-01-01", "0760-01-01"),
            lived(ent("act_parent"), "0750-01-01", "0800-01-01"),
        ],
        vec![],
    );
    let (certainty, message) = only(&w, Rule::ImpossibleParentage);
    assert_eq!(certainty, Certainty::Definite);
    assert!(message.contains("before their parent"), "{message}");
}

#[test]
fn a_child_cannot_arrive_long_after_their_parent_died() {
    let mut child = ent("act_child");
    child.parents = vec!["act_parent".into()];
    let w = world(
        vec![
            lived(child, "0805-01-01", "0860-01-01"),
            lived(ent("act_parent"), "0700-01-01", "0800-01-01"),
        ],
        vec![],
    );
    let (certainty, message) = only(&w, Rule::ImpossibleParentage);
    assert_eq!(certainty, Certainty::Definite);
    assert!(message.contains("after their"), "{message}");
}

/// Within the gestation window, a posthumous child is ordinary and must not be flagged.
#[test]
fn a_posthumous_child_within_gestation_is_fine() {
    let mut child = ent("act_child");
    child.parents = vec!["act_parent".into()];
    let w = world(
        vec![
            lived(child, "0800-05-01", "0860-01-01"),
            lived(ent("act_parent"), "0700-01-01", "0800-01-01"),
        ],
        vec![],
    );
    assert_eq!(check(&w).of_rule(Rule::ImpossibleParentage).count(), 0);
}

#[test]
fn findings_come_back_worst_first() {
    let mut child = ent("act_child");
    child.parents = vec!["act_missing".into()];
    let w = world(
        vec![
            lived(child, "0700-01-01", "0760-01-01"),
            lived(ent("act_ghost"), "0700-01-01", "0750~"),
        ],
        vec![evt("evt_late", "0751-06-01", &["act_ghost"])],
    );
    let report = check(&w);
    assert!(report.findings.len() >= 2);
    let certainties: Vec<_> = report.findings.iter().map(|f| f.certainty).collect();
    let mut sorted = certainties.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(certainties, sorted, "definite findings must lead");
}
