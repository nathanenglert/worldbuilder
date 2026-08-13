//! Fuzzy dates.
//!
//! Real worldbuilding is mostly vague — "sometime in the early Third Age", "about two
//! generations before the war". A date here is never a scalar; it is a constrained
//! window with a nominal point for rendering, and possibly a reference to another
//! date it hangs off.

use serde::{Deserialize, Serialize};

use crate::calendar::{Calendar, CivilDate, Day, Duration, Precision};
use crate::error::{Error, Result};

/// How far a `~` marker widens a date on each side, in days, by the precision it was
/// written at. Per-world so a saga spanning millennia can be vaguer than a chamber piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fuzz {
    pub year: i64,
    pub month: i64,
    pub day: i64,
}

impl Default for Fuzz {
    fn default() -> Self {
        Self { year: 730, month: 30, day: 3 }
    }
}

impl Fuzz {
    fn for_precision(&self, p: Precision) -> i64 {
        match p {
            Precision::Year => self.year,
            Precision::Month => self.month,
            Precision::Day => self.day,
        }
    }

    /// Vagueness for a relative offset takes the granularity the writer used:
    /// `+40y~` is vague in years, `+3d~` is vague in days.
    fn for_offset(&self, d: &Duration) -> i64 {
        if d.years != 0 || d.generations != 0 {
            self.year
        } else if d.months != 0 {
            self.month
        } else {
            self.day
        }
    }
}

/// A date as authored — possibly vague, possibly relative to another date.
///
/// Serialized as its own compact source form (`"0812-04~"`, `"@evt_siege+40y"`) rather
/// than a tagged struct, because world files are meant to be read and hand-edited by
/// the person whose world it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateExpr {
    /// `812`, `0812-04`, `0812-04-17~`
    Civil { date: CivilDate, approx: bool },
    /// `810..815`
    Range { lo: CivilDate, hi: CivilDate },
    /// `>812`
    After { date: CivilDate },
    /// `<812`
    Before { date: CivilDate },
    /// `@evt_sundering+40y`, `@act_aldric.death-2g~`
    Anchor { node: String, offset: Duration, approx: bool },
    /// `?` — genuinely unplaced. Valid, and common early in a world's life.
    Unknown,
}

impl Serialize for DateExpr {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DateExpr {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        crate::parse::parse_date(&raw).map_err(serde::de::Error::custom)
    }
}

impl DateExpr {
    pub fn exact(date: CivilDate) -> Self {
        Self::Civil { date, approx: false }
    }

    /// An absent date. Reads as "unbounded in that direction" at an interval's end,
    /// which is what a missing `from:` or `to:` in a world file should mean.
    pub fn unknown() -> Self {
        Self::Unknown
    }

    /// The node this date hangs off, if any. Drives the resolution DAG.
    pub fn depends_on(&self) -> Option<&str> {
        match self {
            Self::Anchor { node, .. } => Some(node),
            _ => None,
        }
    }

    /// Resolve everything that needs no outside context. Anchors are left to
    /// [`crate::resolve`], which knows what their references resolved to.
    pub fn resolve_local(&self, cal: &Calendar, fuzz: Fuzz) -> Result<Resolved> {
        match self {
            Self::Civil { date, approx } => {
                let (lo, hi) = cal.span(*date)?;
                let pad = if *approx { fuzz.for_precision(date.precision()) } else { 0 };
                Ok(Resolved {
                    earliest: Some(lo.offset(-pad)),
                    latest: Some(hi.offset(pad)),
                    nominal: Some(lo),
                })
            }
            Self::Range { lo, hi } => {
                let (lo_start, _) = cal.span(*lo)?;
                let (_, hi_end) = cal.span(*hi)?;
                if lo_start > hi_end {
                    return Err(Error::InvertedRange { lo: lo.to_string(), hi: hi.to_string() });
                }
                Ok(Resolved {
                    earliest: Some(lo_start),
                    latest: Some(hi_end),
                    nominal: Some(lo_start),
                })
            }
            Self::After { date } => {
                let (_, hi) = cal.span(*date)?;
                let start = hi.offset(1);
                Ok(Resolved { earliest: Some(start), latest: None, nominal: Some(start) })
            }
            Self::Before { date } => {
                let (lo, _) = cal.span(*date)?;
                let end = lo.offset(-1);
                Ok(Resolved { earliest: None, latest: Some(end), nominal: Some(end) })
            }
            Self::Unknown => Ok(Resolved::unknown()),
            Self::Anchor { .. } => Ok(Resolved::unknown()),
        }
    }
}

/// A date reduced to canonical days. `None` on either bound means unbounded in that
/// direction — the writer said "before the Sundering" and nothing more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolved {
    pub earliest: Option<Day>,
    pub latest: Option<Day>,
    /// Best single point, for placing a marker. `None` when the date is unplaceable.
    pub nominal: Option<Day>,
}

impl Resolved {
    pub fn unknown() -> Self {
        Self { earliest: None, latest: None, nominal: None }
    }

    pub fn exact(at: Day) -> Self {
        Self { earliest: Some(at), latest: Some(at), nominal: Some(at) }
    }

    /// True when the date pins down a single day.
    pub fn is_exact(&self) -> bool {
        matches!((self.earliest, self.latest), (Some(a), Some(b)) if a == b)
    }

    pub fn is_unknown(&self) -> bool {
        self.earliest.is_none() && self.latest.is_none()
    }

    /// Width of the uncertainty window in days; `None` if unbounded on either side.
    pub fn uncertainty_days(&self) -> Option<i64> {
        match (self.earliest, self.latest) {
            (Some(a), Some(b)) => Some(b.0 - a.0 + 1),
            _ => None,
        }
    }

    pub fn midpoint(&self) -> Option<Day> {
        match (self.earliest, self.latest) {
            (Some(a), Some(b)) => Some(Day(a.0 + (b.0 - a.0) / 2)),
            _ => self.nominal,
        }
    }

    /// Could this date be at or before `at`? Used by interval containment when the
    /// bound is fuzzy — the honest answer is "possibly", not a hard yes or no.
    pub fn could_precede(&self, at: Day) -> bool {
        self.earliest.is_none_or(|e| e <= at)
    }

    pub fn must_precede(&self, at: Day) -> bool {
        self.latest.is_some_and(|l| l <= at)
    }

    pub fn widen(self, days: i64) -> Self {
        Self {
            earliest: self.earliest.map(|d| d.offset(-days)),
            latest: self.latest.map(|d| d.offset(days)),
            nominal: self.nominal,
        }
    }

    /// Shift every bound by a calendar-aware duration. Unbounded sides stay unbounded.
    pub fn shift(self, cal: &Calendar, dur: &Duration) -> Self {
        Self {
            earliest: self.earliest.map(|d| cal.add_duration(d, dur)),
            latest: self.latest.map(|d| cal.add_duration(d, dur)),
            nominal: self.nominal.map(|d| cal.add_duration(d, dur)),
        }
    }

    /// Resolve an anchored date now that its reference is known.
    pub fn anchored_from(
        base: &Resolved,
        cal: &Calendar,
        offset: &Duration,
        approx: bool,
        fuzz: Fuzz,
    ) -> Self {
        let shifted = base.shift(cal, offset);
        if approx { shifted.widen(fuzz.for_offset(offset)) } else { shifted }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cal() -> Calendar {
        Calendar::gregorian()
    }

    #[test]
    fn a_bare_year_spans_that_year() {
        let cal = cal();
        let r = DateExpr::exact(CivilDate::year(812)).resolve_local(&cal, Fuzz::default()).unwrap();
        // 812 is a leap year, so this is 366 — hence asking the calendar rather than
        // hardcoding, which is the bug this test caught the first time round.
        assert_eq!(r.uncertainty_days(), Some(cal.days_in_year(812)));
        assert!(!r.is_exact());
    }

    #[test]
    fn a_full_date_is_exact() {
        let r = DateExpr::exact(CivilDate::ymd(812, 4, 17))
            .resolve_local(&cal(), Fuzz::default())
            .unwrap();
        assert!(r.is_exact());
        assert_eq!(r.uncertainty_days(), Some(1));
    }

    #[test]
    fn approx_widens_by_the_precision_it_was_written_at() {
        let fuzz = Fuzz { year: 730, month: 30, day: 3 };
        let day = DateExpr::Civil { date: CivilDate::ymd(812, 4, 17), approx: true }
            .resolve_local(&cal(), fuzz)
            .unwrap();
        assert_eq!(day.uncertainty_days(), Some(1 + 6));

        let year = DateExpr::Civil { date: CivilDate::year(812), approx: true }
            .resolve_local(&cal(), fuzz)
            .unwrap();
        assert_eq!(year.uncertainty_days(), Some(cal().days_in_year(812) + 2 * fuzz.year));
    }

    #[test]
    fn open_ended_dates_stay_unbounded() {
        let after = DateExpr::After { date: CivilDate::year(812) }
            .resolve_local(&cal(), Fuzz::default())
            .unwrap();
        assert!(after.latest.is_none());
        assert_eq!(after.uncertainty_days(), None);
        assert_eq!(cal().from_day(after.earliest.unwrap()), CivilDate::ymd(813, 1, 1));

        let before = DateExpr::Before { date: CivilDate::year(812) }
            .resolve_local(&cal(), Fuzz::default())
            .unwrap();
        assert!(before.earliest.is_none());
        assert_eq!(cal().from_day(before.latest.unwrap()), CivilDate::ymd(811, 12, 31));
    }

    #[test]
    fn ranges_cover_both_endpoints_fully() {
        let r = DateExpr::Range { lo: CivilDate::year(810), hi: CivilDate::year(815) }
            .resolve_local(&cal(), Fuzz::default())
            .unwrap();
        assert_eq!(cal().from_day(r.earliest.unwrap()), CivilDate::ymd(810, 1, 1));
        assert_eq!(cal().from_day(r.latest.unwrap()), CivilDate::ymd(815, 12, 31));
    }

    #[test]
    fn inverted_ranges_are_rejected() {
        let err = DateExpr::Range { lo: CivilDate::year(815), hi: CivilDate::year(810) }
            .resolve_local(&cal(), Fuzz::default())
            .unwrap_err();
        assert!(matches!(err, Error::InvertedRange { .. }));
    }

    #[test]
    fn unknown_dates_are_valid_and_unplaceable() {
        let r = DateExpr::Unknown.resolve_local(&cal(), Fuzz::default()).unwrap();
        assert!(r.is_unknown());
        assert!(r.nominal.is_none());
    }

    #[test]
    fn anchoring_carries_uncertainty_forward() {
        let cal = cal();
        let base = DateExpr::Civil { date: CivilDate::year(812), approx: false }
            .resolve_local(&cal, Fuzz::default())
            .unwrap();
        let derived =
            Resolved::anchored_from(&base, &cal, &Duration::years(40), false, Fuzz::default());

        // A vague base stays exactly as vague once shifted — no precision invented.
        assert_eq!(derived.uncertainty_days(), base.uncertainty_days());
        assert_eq!(cal.from_day(derived.earliest.unwrap()), CivilDate::ymd(852, 1, 1));
    }
}
