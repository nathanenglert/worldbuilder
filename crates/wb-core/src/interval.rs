//! Validity intervals and the thirteen Allen relations.
//!
//! Intervals are **half-open**, `[from, to)`. That is not a detail: an annexation in
//! 812 makes Corrath's ownership `[.., 812)` and Vashen's `[812, ..)`, which *meet*
//! exactly. Closed intervals would make the handover day belong to both and every
//! succession would trip the territory-conflict check.

use serde::{Deserialize, Serialize};

use crate::calendar::Day;
use crate::date::Resolved;

/// `None` on a bound means unbounded — the reign has no recorded end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Interval {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Day>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Day>,
}

/// The thirteen ways two intervals can relate. Every consistency rule in the engine
/// is ultimately a statement about which of these are permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllenRelation {
    Before,
    Meets,
    Overlaps,
    Starts,
    During,
    Finishes,
    Equals,
    FinishedBy,
    Contains,
    StartedBy,
    OverlappedBy,
    MetBy,
    After,
}

impl AllenRelation {
    pub fn inverse(self) -> Self {
        use AllenRelation::*;
        match self {
            Before => After,
            Meets => MetBy,
            Overlaps => OverlappedBy,
            Starts => StartedBy,
            During => Contains,
            Finishes => FinishedBy,
            Equals => Equals,
            FinishedBy => Finishes,
            Contains => During,
            StartedBy => Starts,
            OverlappedBy => Overlaps,
            MetBy => Meets,
            After => Before,
        }
    }

    /// Whether the two intervals share at least one day.
    pub fn is_overlapping(self) -> bool {
        use AllenRelation::*;
        !matches!(self, Before | Meets | MetBy | After)
    }
}

/// Whether a fact holds at a given instant. `Maybe` exists because fuzzy dates make
/// it the honest answer — and it is what the map renders as a dashed border.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Containment {
    Yes,
    Maybe,
    No,
}

impl Interval {
    pub fn new(from: Option<Day>, to: Option<Day>) -> Self {
        Self { from, to }
    }

    pub fn everywhen() -> Self {
        Self { from: None, to: None }
    }

    pub fn starting(from: Day) -> Self {
        Self { from: Some(from), to: None }
    }

    pub fn bounded(from: Day, to: Day) -> Self {
        Self { from: Some(from), to: Some(to) }
    }

    /// Build from a pair of bounds that are inclusive at *both* ends, as a fuzzy date's
    /// `earliest`/`latest` are.
    ///
    /// Without this, an exactly-dated event becomes `[x, x)` — an empty interval that
    /// overlaps nothing, and silently vanishes from every range query.
    pub fn inclusive(from: Option<Day>, to: Option<Day>) -> Self {
        Self { from, to: to.map(|d| d.offset(1)) }
    }

    fn lo(&self) -> i64 {
        self.from.map_or(i64::MIN, |d| d.0)
    }

    fn hi(&self) -> i64 {
        self.to.map_or(i64::MAX, |d| d.0)
    }

    pub fn is_empty(&self) -> bool {
        self.lo() >= self.hi()
    }

    pub fn contains(&self, at: Day) -> bool {
        self.lo() <= at.0 && at.0 < self.hi()
    }

    /// True when `other` lies entirely inside `self`. An empty `other` is covered by
    /// anything, since it asserts nothing.
    pub fn covers(&self, other: &Interval) -> bool {
        other.is_empty() || (self.lo() <= other.lo() && other.hi() <= self.hi())
    }

    pub fn overlaps(&self, other: &Interval) -> bool {
        !self.is_empty() && !other.is_empty() && self.lo() < other.hi() && other.lo() < self.hi()
    }

    pub fn intersect(&self, other: &Interval) -> Option<Interval> {
        let lo = self.lo().max(other.lo());
        let hi = self.hi().min(other.hi());
        if lo >= hi {
            return None;
        }
        Some(Interval {
            from: (lo != i64::MIN).then_some(Day(lo)),
            to: (hi != i64::MAX).then_some(Day(hi)),
        })
    }

    /// Duration in days; `None` when unbounded.
    pub fn length(&self) -> Option<i64> {
        match (self.from, self.to) {
            (Some(a), Some(b)) => Some((b.0 - a.0).max(0)),
            _ => None,
        }
    }

    /// Classify against another interval. Assumes both are non-empty; empty intervals
    /// are ordered by their start and never reported as overlapping.
    pub fn relate(&self, other: &Interval) -> AllenRelation {
        use AllenRelation::*;
        let (a1, a2) = (self.lo(), self.hi());
        let (b1, b2) = (other.lo(), other.hi());

        if self.is_empty() || other.is_empty() {
            return if a1 < b1 {
                Before
            } else if a1 > b1 {
                After
            } else {
                Equals
            };
        }
        if a2 < b1 {
            return Before;
        }
        if a2 == b1 {
            return Meets;
        }
        if a1 > b2 {
            return After;
        }
        if a1 == b2 {
            return MetBy;
        }
        if a1 == b1 && a2 == b2 {
            return Equals;
        }
        if a1 == b1 {
            return if a2 < b2 { Starts } else { StartedBy };
        }
        if a2 == b2 {
            return if a1 > b1 { Finishes } else { FinishedBy };
        }
        if a1 < b1 && a2 > b2 {
            return Contains;
        }
        if a1 > b1 && a2 < b2 {
            return During;
        }
        if a1 < b1 { Overlaps } else { OverlappedBy }
    }
}

/// An interval whose ends are fuzzy dates: a core that definitely holds, wrapped in a
/// window where it might. Renderers draw the first solid and the difference faded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuzzyInterval {
    pub certain: Interval,
    pub possible: Interval,
}

impl FuzzyInterval {
    /// Build from the resolved start and end of a fact. The certain core runs from the
    /// *latest* possible start to the *earliest* possible end; the possible window is
    /// the reverse. A fact vaguer than its own duration has an empty core, which is
    /// correct — nothing about it is definite.
    pub fn new(start: &Resolved, end: &Resolved) -> Self {
        Self {
            certain: Interval::new(start.latest, end.earliest),
            possible: Interval::new(start.earliest, end.latest),
        }
    }

    pub fn at(&self, day: Day) -> Containment {
        if self.certain.contains(day) {
            Containment::Yes
        } else if self.possible.contains(day) {
            Containment::Maybe
        } else {
            Containment::No
        }
    }

    pub fn is_sharp(&self) -> bool {
        self.certain == self.possible
    }
}

/// Every instant where something could change, sorted and deduplicated.
///
/// This is the trick that keeps the scrubber smooth: dragging *between* two change
/// points cannot alter what the map shows, so no requery is needed until one is crossed.
pub fn change_points(intervals: impl IntoIterator<Item = Interval>) -> Vec<Day> {
    let mut points: Vec<Day> =
        intervals.into_iter().flat_map(|i| [i.from, i.to]).flatten().collect();
    points.sort_unstable();
    points.dedup();
    points
}

#[cfg(test)]
mod tests {
    use super::AllenRelation::*;
    use super::*;

    fn iv(from: i64, to: i64) -> Interval {
        Interval::bounded(Day(from), Day(to))
    }

    #[test]
    fn half_open_intervals_meet_without_overlapping() {
        let corrath = iv(0, 812);
        let vashen = Interval::starting(Day(812));
        assert_eq!(corrath.relate(&vashen), Meets);
        assert!(!corrath.overlaps(&vashen));
        // The handover day belongs to exactly one owner.
        assert!(!corrath.contains(Day(812)));
        assert!(vashen.contains(Day(812)));
    }

    #[test]
    fn covers_all_thirteen_relations() {
        let base = iv(10, 20);
        let cases = [
            (iv(0, 5), Before),
            (iv(0, 10), Meets),
            (iv(5, 15), Overlaps),
            (iv(10, 15), Starts),
            (iv(12, 18), During),
            (iv(15, 20), Finishes),
            (iv(10, 20), Equals),
            (iv(10, 25), StartedBy),
            (iv(5, 25), Contains),
            (iv(15, 25), OverlappedBy),
            (iv(20, 25), MetBy),
            (iv(25, 30), After),
        ];
        for (other, expected) in cases {
            assert_eq!(other.relate(&base), expected, "{other:?} vs {base:?}");
            assert_eq!(base.relate(&other), expected.inverse(), "inverse of {expected:?}");
        }
        assert_eq!(iv(10, 20).relate(&iv(5, 20)), Finishes);
    }

    #[test]
    fn overlap_agrees_with_the_relation() {
        let base = iv(10, 20);
        for other in [iv(0, 5), iv(0, 10), iv(5, 15), iv(12, 18), iv(20, 25), iv(25, 30)] {
            assert_eq!(other.overlaps(&base), other.relate(&base).is_overlapping());
        }
    }

    #[test]
    fn unbounded_intervals_behave() {
        let forever = Interval::everywhen();
        assert!(forever.contains(Day(-999_999)));
        assert!(forever.contains(Day(999_999)));
        assert_eq!(forever.length(), None);
        assert_eq!(iv(10, 20).relate(&forever), During);

        let open_ended = Interval::starting(Day(100));
        assert!(open_ended.contains(Day(1_000_000)));
        assert!(!open_ended.contains(Day(99)));
    }

    #[test]
    fn an_exactly_dated_thing_is_still_findable() {
        // Regression: an exact date has earliest == latest, and `[x, x)` is empty, so
        // building a query interval from it directly made precisely-dated events
        // invisible to every range query.
        let naive = Interval::new(Some(Day(100)), Some(Day(100)));
        assert!(naive.is_empty());
        assert!(!naive.overlaps(&iv(0, 200)));

        let correct = Interval::inclusive(Some(Day(100)), Some(Day(100)));
        assert!(!correct.is_empty());
        assert!(correct.contains(Day(100)));
        assert!(correct.overlaps(&iv(0, 200)));

        // Unbounded ends stay unbounded rather than gaining a phantom day.
        assert_eq!(Interval::inclusive(Some(Day(5)), None), Interval::starting(Day(5)));
    }

    #[test]
    fn containment_handles_unbounded_and_empty() {
        assert!(iv(0, 100).covers(&iv(10, 90)));
        assert!(iv(0, 100).covers(&iv(0, 100)), "an interval covers itself");
        assert!(!iv(0, 100).covers(&iv(50, 150)));
        assert!(!iv(50, 150).covers(&iv(0, 100)));

        assert!(Interval::everywhen().covers(&iv(-9999, 9999)));
        assert!(!iv(0, 100).covers(&Interval::everywhen()));
        assert!(Interval::starting(Day(0)).covers(&iv(500, 600)));

        // An empty interval asserts nothing, so nothing can contradict it.
        assert!(iv(0, 10).covers(&iv(50, 50)));
    }

    #[test]
    fn intersection_drops_sentinels() {
        assert_eq!(iv(0, 20).intersect(&iv(10, 30)), Some(iv(10, 20)));
        assert_eq!(iv(0, 10).intersect(&iv(10, 20)), None, "meeting intervals share no day");
        assert_eq!(
            Interval::everywhen().intersect(&iv(5, 9)),
            Some(iv(5, 9)),
            "unbounded sides must not leak i64 sentinels"
        );
    }

    #[test]
    fn fuzzy_intervals_separate_certain_from_possible() {
        let start =
            Resolved { earliest: Some(Day(0)), latest: Some(Day(10)), nominal: Some(Day(0)) };
        let end =
            Resolved { earliest: Some(Day(90)), latest: Some(Day(100)), nominal: Some(Day(90)) };
        let f = FuzzyInterval::new(&start, &end);

        assert_eq!(f.at(Day(50)), Containment::Yes);
        assert_eq!(f.at(Day(5)), Containment::Maybe, "inside the vague opening");
        assert_eq!(f.at(Day(95)), Containment::Maybe, "inside the vague closing");
        assert_eq!(f.at(Day(-1)), Containment::No);
        assert_eq!(f.at(Day(200)), Containment::No);
        assert!(!f.is_sharp());
    }

    #[test]
    fn a_fact_vaguer_than_its_span_has_no_certain_core() {
        let start =
            Resolved { earliest: Some(Day(0)), latest: Some(Day(100)), nominal: Some(Day(0)) };
        let end =
            Resolved { earliest: Some(Day(50)), latest: Some(Day(150)), nominal: Some(Day(50)) };
        let f = FuzzyInterval::new(&start, &end);
        assert!(f.certain.is_empty());
        assert_eq!(f.at(Day(75)), Containment::Maybe);
    }

    #[test]
    fn change_points_are_sorted_and_deduped() {
        let points = change_points([iv(0, 812), Interval::starting(Day(812)), iv(400, 900)]);
        assert_eq!(points, vec![Day(0), Day(400), Day(812), Day(900)]);
    }
}
