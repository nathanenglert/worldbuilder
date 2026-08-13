//! The scenario from DESIGN.md, run end to end through the public API.
//!
//! Corrath is sovereign until Vashen takes it at the Siege of Marrow. Aldric Vane,
//! Duke of Corrath, is born in 771 and dies "around 811" — vaguely enough that whether
//! he lived to see the siege is genuinely an open question, and the engine says so
//! rather than guessing.

use std::collections::BTreeMap;

use wb_core::{
    Calendar, CivilDate, Containment, DateExpr, Day, FuzzyInterval, Month, Resolved, Resolver,
    change_points, parse_date,
};

/// Twelve thirty-day months, no leap rule. Nothing like Earth's, which is the point.
fn vashen_reckoning() -> Calendar {
    const MONTHS: [&str; 12] = [
        "Frostwane",
        "Seedfall",
        "Greening",
        "Verdant",
        "Highsun",
        "Emberlong",
        "Goldfell",
        "Harvestide",
        "Duskmere",
        "Rimefall",
        "Longdark",
        "Yearsend",
    ];
    Calendar::new("Vashen Reckoning", MONTHS.iter().map(|n| Month::new(*n, 30)).collect()).unwrap()
}

fn dates(pairs: &[(&str, &str)]) -> BTreeMap<String, DateExpr> {
    pairs.iter().map(|(id, src)| (id.to_string(), parse_date(src).unwrap())).collect()
}

/// The siege is known only to the month; Aldric's death only to "around 811".
fn world() -> BTreeMap<String, DateExpr> {
    dates(&[
        ("evt_founding", "0500-01-01"),
        ("evt_siege", "0812-04"),
        ("act_aldric.birth", "0771-06-12"),
        ("act_aldric.death", "0811~"),
    ])
}

struct Territory {
    corrath: FuzzyInterval,
    vashen: FuzzyInterval,
}

/// One attribute — who owns `ter_corrath` — as two intervals meeting at the siege.
fn ownership(siege: &Resolved) -> Territory {
    Territory {
        corrath: FuzzyInterval::new(&Resolved::unknown(), siege),
        vashen: FuzzyInterval::new(siege, &Resolved::unknown()),
    }
}

#[test]
fn scrubbing_across_the_annexation_hands_the_territory_over() {
    let cal = vashen_reckoning();
    let resolved = Resolver::new(&cal).resolve_all(&world()).unwrap();
    let ter = ownership(&resolved["evt_siege"]);
    let day = |y, m, d| cal.to_day(CivilDate::ymd(y, m, d)).unwrap();

    assert_eq!(ter.corrath.at(day(806, 1, 1)), Containment::Yes);
    assert_eq!(ter.vashen.at(day(806, 1, 1)), Containment::No);

    assert_eq!(ter.corrath.at(day(820, 1, 1)), Containment::No);
    assert_eq!(ter.vashen.at(day(820, 1, 1)), Containment::Yes);
}

#[test]
fn a_month_precise_siege_leaves_the_border_uncertain_for_exactly_that_month() {
    let cal = vashen_reckoning();
    let resolved = Resolver::new(&cal).resolve_all(&world()).unwrap();
    let ter = ownership(&resolved["evt_siege"]);
    let day = |y, m, d| cal.to_day(CivilDate::ymd(y, m, d)).unwrap();

    // Inside Verdant 812 neither claim is settled — this is the dashed border.
    assert_eq!(ter.corrath.at(day(812, 4, 15)), Containment::Maybe);
    assert_eq!(ter.vashen.at(day(812, 4, 15)), Containment::Maybe);

    // The day before the month opens, and the day after it closes, are settled.
    assert_eq!(ter.corrath.at(day(812, 3, 30)), Containment::Yes);
    assert_eq!(ter.vashen.at(day(812, 5, 1)), Containment::Yes);

    assert!(!ter.corrath.is_sharp());
}

#[test]
fn an_exact_handover_leaves_no_day_owned_twice() {
    let cal = vashen_reckoning();
    let resolved = Resolver::new(&cal).resolve_all(&dates(&[("evt_siege", "0812-04-17")])).unwrap();
    let ter = ownership(&resolved["evt_siege"]);
    let handover = cal.to_day(CivilDate::ymd(812, 4, 17)).unwrap();

    assert_eq!(ter.corrath.at(handover), Containment::No);
    assert_eq!(ter.vashen.at(handover), Containment::Yes);
    assert_eq!(ter.corrath.at(handover.offset(-1)), Containment::Yes);
    assert!(ter.corrath.is_sharp() && ter.vashen.is_sharp());
}

#[test]
fn a_lifespan_violation_is_just_interval_containment() {
    let cal = vashen_reckoning();
    let resolved = Resolver::new(&cal).resolve_all(&world()).unwrap();
    let life = FuzzyInterval::new(&resolved["act_aldric.birth"], &resolved["act_aldric.death"]);
    let day = |y, m, d| cal.to_day(CivilDate::ymd(y, m, d)).unwrap();

    assert_eq!(life.at(day(800, 1, 1)), Containment::Yes, "alive, no question");
    assert_eq!(life.at(day(760, 1, 1)), Containment::No, "not yet born — hard violation");
    assert_eq!(life.at(day(850, 1, 1)), Containment::No, "long dead — hard violation");

    // He died "around 811" and the siege is 812. The engine refuses to decide.
    assert_eq!(
        life.at(day(812, 4, 15)),
        Containment::Maybe,
        "a soft warning, not an error — this is exactly the case a rigid model gets wrong"
    );
}

#[test]
fn retiming_the_founding_drags_the_siege_and_the_border_with_it() {
    let cal = vashen_reckoning();
    let resolver = Resolver::new(&cal);

    let siege_year = |founding: &str| {
        let mut w = dates(&[("evt_siege", "@evt_founding+312y")]);
        w.insert("evt_founding".into(), parse_date(founding).unwrap());
        let resolved = resolver.resolve_all(&w).unwrap();
        cal.from_day(resolved["evt_siege"].nominal.unwrap()).year
    };

    assert_eq!(siege_year("0500-01-01"), 812);
    assert_eq!(siege_year("0450-01-01"), 762);
    assert_eq!(siege_year("0640-01-01"), 952);
}

#[test]
fn change_points_bound_how_often_the_map_must_requery() {
    let cal = vashen_reckoning();
    let resolved = Resolver::new(&cal).resolve_all(&world()).unwrap();
    let ter = ownership(&resolved["evt_siege"]);

    let points = change_points([
        ter.corrath.possible,
        ter.corrath.certain,
        ter.vashen.possible,
        ter.vashen.certain,
    ]);

    // Only the two edges of the uncertain month matter; every scrub position between
    // them renders identically, so dragging across a decade is two queries, not 3,600.
    assert_eq!(points.len(), 2);
    assert_eq!(cal.from_day(points[0]), CivilDate::ymd(812, 4, 1));
    assert_eq!(cal.from_day(points[1]), CivilDate::ymd(812, 4, 30));
}

#[test]
fn the_engine_never_invents_precision_it_was_not_given() {
    let cal = vashen_reckoning();
    let resolved = Resolver::new(&cal).resolve_all(&world()).unwrap();

    assert_eq!(resolved["evt_siege"].uncertainty_days(), Some(30), "a month stays a month");
    assert!(resolved["act_aldric.birth"].is_exact());
    assert!(resolved["act_aldric.death"].uncertainty_days().unwrap() > 365);
}

#[test]
fn day_zero_is_the_epoch_regardless_of_calendar() {
    let cal = vashen_reckoning();
    assert_eq!(cal.to_day(CivilDate::ymd(0, 1, 1)).unwrap(), Day(0));
    assert_eq!(cal.days_in_year(812), 360);
}
