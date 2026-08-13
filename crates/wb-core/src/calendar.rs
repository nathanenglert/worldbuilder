//! Calendars, and the canonical scalar every date reduces to.
//!
//! Dates are *stored* as [`Day`] — an integer count from the world's epoch — and only
//! *rendered* through a [`Calendar`]. That split is deliberate: a writer can rename
//! months, add an era, or change a leap rule at any point without touching a single
//! stored date.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Canonical scalar time: days elapsed since year 0, month 1, day 1 of this world's
/// calendar. Negative values are prehistory, which worlds have a lot of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Day(pub i64);

impl Day {
    pub const MIN: Day = Day(i64::MIN / 4);
    pub const MAX: Day = Day(i64::MAX / 4);

    pub fn offset(self, days: i64) -> Day {
        Day(self.0.saturating_add(days))
    }
}

impl std::fmt::Display for Day {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "day {}", self.0)
    }
}

/// How precisely a date was stated. Precision *is* uncertainty: a writer who says
/// "812" has named a whole year, so the date spans it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    Year,
    Month,
    Day,
}

/// A date as a human writes it, at whatever precision they had.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CivilDate {
    pub year: i64,
    /// 1-indexed. `None` means the writer only named a year.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub month: Option<u8>,
    /// 1-indexed. `None` means the writer only named a year or a month.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day: Option<u16>,
}

impl CivilDate {
    pub fn year(year: i64) -> Self {
        Self { year, month: None, day: None }
    }

    pub fn ym(year: i64, month: u8) -> Self {
        Self { year, month: Some(month), day: None }
    }

    pub fn ymd(year: i64, month: u8, day: u16) -> Self {
        Self { year, month: Some(month), day: Some(day) }
    }

    pub fn precision(&self) -> Precision {
        match (self.month, self.day) {
            (Some(_), Some(_)) => Precision::Day,
            (Some(_), None) => Precision::Month,
            _ => Precision::Year,
        }
    }
}

impl std::fmt::Display for CivilDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}", self.year)?;
        if let Some(m) = self.month {
            write!(f, "-{m:02}")?;
        }
        if let Some(d) = self.day {
            write!(f, "-{d:02}")?;
        }
        Ok(())
    }
}

/// A span of time expressed in calendar units rather than raw days, because
/// "40 years later" must land on the same month and day, whatever the leap rules did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duration {
    #[serde(default)]
    pub years: i64,
    #[serde(default)]
    pub months: i64,
    #[serde(default)]
    pub days: i64,
    /// Generations are a unit writers reach for constantly ("two generations before
    /// the war"). Length is per-world, defaulting to 25 years.
    #[serde(default)]
    pub generations: i64,
}

impl Duration {
    pub fn years(n: i64) -> Self {
        Self { years: n, ..Default::default() }
    }

    pub fn days(n: i64) -> Self {
        Self { days: n, ..Default::default() }
    }

    pub fn generations(n: i64) -> Self {
        Self { generations: n, ..Default::default() }
    }

    pub fn is_zero(&self) -> bool {
        *self == Self::default()
    }

    pub fn negated(self) -> Self {
        Self {
            years: -self.years,
            months: -self.months,
            days: -self.days,
            generations: -self.generations,
        }
    }
}

impl std::fmt::Display for Duration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts =
            [(self.years, 'y'), (self.months, 'm'), (self.days, 'd'), (self.generations, 'g')];
        let mut wrote = false;
        for (n, unit) in parts {
            if n != 0 {
                write!(f, "{n}{unit}")?;
                wrote = true;
            }
        }
        if !wrote {
            write!(f, "0d")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Month {
    pub name: String,
    pub days: u16,
}

impl Month {
    pub fn new(name: impl Into<String>, days: u16) -> Self {
        Self { name: name.into(), days }
    }
}

/// Leap rule in the Gregorian shape: a year gains days when divisible by `every`,
/// unless divisible by `except`, unless also divisible by `also`.
///
/// The fast leap-counting path assumes `also` divides `except` divides `every`
/// (as 400/100/4 does). Rules that break that nesting still render correctly but
/// should be checked with [`Calendar::audit_leap_rule`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeapRule {
    pub every: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub except: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub also: Option<i64>,
    /// 1-indexed month that gains the days.
    pub month: usize,
    #[serde(default = "one_day")]
    pub extra_days: u16,
}

fn one_day() -> u16 {
    1
}

/// A named counting epoch — "AR", "Third Age", "Before the Sundering".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Era {
    pub name: String,
    #[serde(default)]
    pub abbr: String,
    /// First absolute year covered by this era.
    pub starts: i64,
    /// Set for eras that count backwards toward their end (BC-style).
    #[serde(default)]
    pub descending: bool,
}

fn default_generation_years() -> i64 {
    25
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Calendar {
    pub name: String,
    pub months: Vec<Month>,
    #[serde(default)]
    pub weekdays: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leap: Option<LeapRule>,
    #[serde(default)]
    pub eras: Vec<Era>,
    #[serde(default = "default_generation_years")]
    pub generation_years: i64,
}

/// Count of integers `m` where `0 <= m < y` (or negated, for `y < 0`) with `m % k == 0`.
fn multiples_below(y: i64, k: i64) -> i64 {
    if k <= 0 {
        return 0;
    }
    match y {
        0 => 0,
        y if y > 0 => (y - 1).div_euclid(k) + 1,
        y => -((-y) / k),
    }
}

impl Calendar {
    pub fn new(name: impl Into<String>, months: Vec<Month>) -> Result<Self> {
        let cal = Self {
            name: name.into(),
            months,
            weekdays: Vec::new(),
            leap: None,
            eras: Vec::new(),
            generation_years: default_generation_years(),
        };
        cal.validate()?;
        Ok(cal)
    }

    /// Earth's calendar, useful as a default and as the reference for tests where
    /// the expected answers are independently known.
    pub fn gregorian() -> Self {
        Self {
            name: "Gregorian".into(),
            months: vec![
                Month::new("January", 31),
                Month::new("February", 28),
                Month::new("March", 31),
                Month::new("April", 30),
                Month::new("May", 31),
                Month::new("June", 30),
                Month::new("July", 31),
                Month::new("August", 31),
                Month::new("September", 30),
                Month::new("October", 31),
                Month::new("November", 30),
                Month::new("December", 31),
            ],
            weekdays: [
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            leap: Some(LeapRule {
                every: 4,
                except: Some(100),
                also: Some(400),
                month: 2,
                extra_days: 1,
            }),
            eras: Vec::new(),
            generation_years: default_generation_years(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.months.is_empty() {
            return Err(Error::EmptyCalendar);
        }
        if let Some(r) = &self.leap
            && (r.month == 0 || r.month > self.months.len())
        {
            return Err(Error::BadLeapMonth { month: r.month, count: self.months.len() });
        }
        Ok(())
    }

    /// True when the leap rule's divisors nest, which the O(1) leap count relies on.
    pub fn audit_leap_rule(&self) -> bool {
        let Some(r) = &self.leap else { return true };
        let nested_except = r.except.is_none_or(|e| e > 0 && e % r.every == 0);
        let nested_also = match (r.except, r.also) {
            (Some(e), Some(a)) => a > 0 && a % e == 0,
            (None, Some(_)) => false,
            _ => true,
        };
        nested_except && nested_also
    }

    pub fn month_count(&self) -> usize {
        self.months.len()
    }

    fn extra_days(&self) -> i64 {
        self.leap.as_ref().map_or(0, |r| r.extra_days as i64)
    }

    fn common_year_days(&self) -> i64 {
        self.months.iter().map(|m| m.days as i64).sum()
    }

    pub fn is_leap(&self, year: i64) -> bool {
        let Some(r) = &self.leap else { return false };
        if r.every <= 0 || year.rem_euclid(r.every) != 0 {
            return false;
        }
        if let Some(e) = r.except
            && e > 0
            && year.rem_euclid(e) == 0
        {
            return r.also.is_some_and(|a| a > 0 && year.rem_euclid(a) == 0);
        }
        true
    }

    /// Leap years in `[0, year)`, negated for years before the epoch. O(1).
    fn leap_count(&self, year: i64) -> i64 {
        let Some(r) = &self.leap else { return 0 };
        let mut n = multiples_below(year, r.every);
        if let Some(e) = r.except {
            n -= multiples_below(year, e);
        }
        if let Some(a) = r.also {
            n += multiples_below(year, a);
        }
        n
    }

    pub fn days_in_year(&self, year: i64) -> i64 {
        self.common_year_days() + if self.is_leap(year) { self.extra_days() } else { 0 }
    }

    pub fn days_in_month(&self, year: i64, month: u8) -> Result<u16> {
        let idx = self.month_index(month)?;
        let mut len = self.months[idx].days;
        if let Some(r) = &self.leap
            && r.month == month as usize
            && self.is_leap(year)
        {
            len += r.extra_days;
        }
        Ok(len)
    }

    pub fn month_name(&self, month: u8) -> Result<&str> {
        Ok(&self.months[self.month_index(month)?].name)
    }

    fn month_index(&self, month: u8) -> Result<usize> {
        if month == 0 || month as usize > self.months.len() {
            return Err(Error::NoSuchMonth {
                calendar: self.name.clone(),
                month,
                count: self.months.len(),
            });
        }
        Ok(month as usize - 1)
    }

    fn year_start(&self, year: i64) -> i64 {
        year * self.common_year_days() + self.leap_count(year) * self.extra_days()
    }

    /// Convert a civil date to the canonical scalar. Missing month/day components
    /// resolve to the first of the period; use [`Calendar::span`] for the full range.
    pub fn to_day(&self, date: CivilDate) -> Result<Day> {
        let month = date.month.unwrap_or(1);
        let day = date.day.unwrap_or(1);
        let len = self.days_in_month(date.year, month)?;
        if day == 0 || day > len {
            return Err(Error::NoSuchDay {
                year: date.year,
                month_name: self.month_name(month)?.to_string(),
                day,
                len,
            });
        }

        let mut total = self.year_start(date.year);
        for m in 1..month {
            total += self.days_in_month(date.year, m)? as i64;
        }
        Ok(Day(total + day as i64 - 1))
    }

    /// The inclusive first and last day covered by a civil date at its stated precision.
    /// This is where partial dates become uncertainty for free: "812" spans its year.
    pub fn span(&self, date: CivilDate) -> Result<(Day, Day)> {
        let first = self.to_day(date)?;
        let width = match date.precision() {
            Precision::Day => 1,
            Precision::Month => self.days_in_month(date.year, date.month.unwrap_or(1))? as i64,
            Precision::Year => self.days_in_year(date.year),
        };
        Ok((first, Day(first.0 + width - 1)))
    }

    pub fn from_day(&self, at: Day) -> CivilDate {
        let common = self.common_year_days().max(1);
        let mut year = at.0.div_euclid(common);

        // The estimate ignores accumulated leap days, so walk it into place. Drift is
        // roughly one year per century of history, so this is a handful of steps.
        loop {
            let start = self.year_start(year);
            if at.0 < start {
                year -= 1;
                continue;
            }
            if at.0 >= start + self.days_in_year(year) {
                year += 1;
                continue;
            }
            break;
        }

        let mut rem = at.0 - self.year_start(year);
        let mut month: u8 = 1;
        while (month as usize) < self.months.len() {
            let len = self.days_in_month(year, month).unwrap_or(0) as i64;
            if rem < len {
                break;
            }
            rem -= len;
            month += 1;
        }
        CivilDate::ymd(year, month, rem as u16 + 1)
    }

    /// Calendar-aware arithmetic: years and months move by name, days by count.
    /// Landing on a day the target month lacks clamps to its end.
    pub fn add_duration(&self, at: Day, dur: &Duration) -> Day {
        let years = dur.years + dur.generations * self.generation_years;
        if years == 0 && dur.months == 0 {
            return at.offset(dur.days);
        }

        let civil = self.from_day(at);
        let n = self.months.len() as i64;
        let shifted = civil.month.unwrap_or(1) as i64 - 1 + dur.months;
        let year = civil.year + years + shifted.div_euclid(n);
        let month = (shifted.rem_euclid(n) + 1) as u8;

        let len = self.days_in_month(year, month).unwrap_or(1);
        let day = civil.day.unwrap_or(1).min(len);

        match self.to_day(CivilDate::ymd(year, month, day)) {
            Ok(base) => base.offset(dur.days),
            Err(_) => at.offset(dur.days),
        }
    }

    pub fn weekday(&self, at: Day) -> Option<&str> {
        if self.weekdays.is_empty() {
            return None;
        }
        let idx = at.0.rem_euclid(self.weekdays.len() as i64) as usize;
        Some(&self.weekdays[idx])
    }

    pub fn era_for(&self, year: i64) -> Option<&Era> {
        self.eras.iter().filter(|e| year >= e.starts).max_by_key(|e| e.starts)
    }

    /// Zero-padded numeric form: `0812-04-17`.
    pub fn format_numeric(&self, at: Day) -> String {
        self.from_day(at).to_string()
    }

    /// Reader-facing form: `17 April, 812 AR`.
    pub fn format_long(&self, at: Day) -> String {
        let c = self.from_day(at);
        let month = c.month.and_then(|m| self.month_name(m).ok()).unwrap_or("?");
        match self.era_for(c.year) {
            Some(era) => {
                let n =
                    if era.descending { era.starts - c.year + 1 } else { c.year - era.starts + 1 };
                let label = if era.abbr.is_empty() { &era.name } else { &era.abbr };
                format!("{} {}, {} {}", c.day.unwrap_or(1), month, n, label)
            }
            None => format!("{} {}, {}", c.day.unwrap_or(1), month, c.year),
        }
    }
}

impl Default for Calendar {
    fn default() -> Self {
        Self::gregorian()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10-month, 36-day world with a 5-year leap cycle — deliberately unlike Earth,
    /// to catch anywhere Gregorian assumptions leaked into the arithmetic.
    fn odd_world() -> Calendar {
        Calendar {
            name: "Vashen Reckoning".into(),
            months: (1..=10).map(|i| Month::new(format!("Month{i}"), 36)).collect(),
            weekdays: vec!["Ald".into(), "Bren".into(), "Corr".into()],
            leap: Some(LeapRule { every: 5, except: None, also: None, month: 10, extra_days: 2 }),
            eras: vec![Era {
                name: "After Reckoning".into(),
                abbr: "AR".into(),
                starts: 0,
                descending: false,
            }],
            generation_years: 30,
        }
    }

    #[test]
    fn epoch_is_day_zero() {
        let cal = Calendar::gregorian();
        assert_eq!(cal.to_day(CivilDate::ymd(0, 1, 1)).unwrap(), Day(0));
    }

    #[test]
    fn gregorian_leap_years_match_earth() {
        let cal = Calendar::gregorian();
        for (year, expected) in
            [(1900, false), (1996, true), (2000, true), (2023, false), (2024, true)]
        {
            assert_eq!(cal.is_leap(year), expected, "year {year}");
        }
        assert_eq!(cal.days_in_month(2024, 2).unwrap(), 29);
        assert_eq!(cal.days_in_month(1900, 2).unwrap(), 28);
    }

    #[test]
    fn leap_count_agrees_with_brute_force() {
        let cal = Calendar::gregorian();
        for year in [-800, -13, 0, 1, 4, 101, 1000, 2026] {
            let brute: i64 = if year >= 0 {
                (0..year).filter(|y| cal.is_leap(*y)).count() as i64
            } else {
                -((year..0).filter(|y| cal.is_leap(*y)).count() as i64)
            };
            assert_eq!(cal.leap_count(year), brute, "year {year}");
        }
    }

    #[test]
    fn day_conversion_round_trips_across_eras() {
        for cal in [Calendar::gregorian(), odd_world()] {
            for offset in [-900_000, -4001, -1, 0, 1, 365, 100_000, 1_500_000] {
                let day = Day(offset);
                let civil = cal.from_day(day);
                assert_eq!(cal.to_day(civil).unwrap(), day, "{} at {offset}", cal.name);
            }
        }
    }

    #[test]
    fn partial_dates_span_their_period() {
        let cal = Calendar::gregorian();
        let (lo, hi) = cal.span(CivilDate::year(2024)).unwrap();
        assert_eq!(cal.from_day(lo), CivilDate::ymd(2024, 1, 1));
        assert_eq!(cal.from_day(hi), CivilDate::ymd(2024, 12, 31));
        assert_eq!(hi.0 - lo.0 + 1, 366, "2024 is a leap year");

        let (lo, hi) = cal.span(CivilDate::ym(2023, 2)).unwrap();
        assert_eq!(hi.0 - lo.0 + 1, 28);

        let (lo, hi) = cal.span(CivilDate::ymd(2023, 2, 14)).unwrap();
        assert_eq!(lo, hi);
    }

    #[test]
    fn odd_world_year_length_accounts_for_leap() {
        let cal = odd_world();
        assert_eq!(cal.days_in_year(1), 360);
        assert_eq!(cal.days_in_year(5), 362);
        assert_eq!(cal.days_in_month(5, 10).unwrap(), 38);
    }

    #[test]
    fn adding_years_preserves_month_and_day() {
        let cal = odd_world();
        let start = cal.to_day(CivilDate::ymd(812, 4, 17)).unwrap();
        let later = cal.add_duration(start, &Duration::years(40));
        assert_eq!(cal.from_day(later), CivilDate::ymd(852, 4, 17));
    }

    #[test]
    fn generations_use_the_worlds_own_length() {
        let cal = odd_world(); // 30-year generations
        let start = cal.to_day(CivilDate::ymd(800, 1, 1)).unwrap();
        let later = cal.add_duration(start, &Duration::generations(2));
        assert_eq!(cal.from_day(later).year, 860);
    }

    #[test]
    fn month_overflow_rolls_the_year() {
        let cal = odd_world(); // 10 months
        let start = cal.to_day(CivilDate::ymd(800, 9, 1)).unwrap();
        let later = cal.add_duration(start, &Duration { months: 4, ..Default::default() });
        assert_eq!(cal.from_day(later), CivilDate::ymd(801, 3, 1));
    }

    #[test]
    fn short_month_clamps_rather_than_overflowing() {
        let cal = Calendar::gregorian();
        let start = cal.to_day(CivilDate::ymd(2024, 1, 31)).unwrap();
        let later = cal.add_duration(start, &Duration { months: 1, ..Default::default() });
        assert_eq!(cal.from_day(later), CivilDate::ymd(2024, 2, 29));
    }

    #[test]
    fn negative_durations_walk_backwards() {
        let cal = Calendar::gregorian();
        let start = cal.to_day(CivilDate::ymd(812, 4, 17)).unwrap();
        let earlier = cal.add_duration(start, &Duration::years(40).negated());
        assert_eq!(cal.from_day(earlier), CivilDate::ymd(772, 4, 17));
    }

    #[test]
    fn rejects_impossible_days() {
        let cal = Calendar::gregorian();
        assert!(cal.to_day(CivilDate::ymd(2023, 2, 29)).is_err());
        assert!(cal.to_day(CivilDate::ymd(2023, 13, 1)).is_err());
        assert!(cal.to_day(CivilDate::ymd(2023, 4, 31)).is_err());
        assert!(cal.to_day(CivilDate::ymd(2024, 2, 29)).is_ok());
    }

    #[test]
    fn calendar_validation_catches_bad_definitions() {
        assert_eq!(Calendar::new("Void", vec![]).unwrap_err(), Error::EmptyCalendar);

        let bad = Calendar {
            leap: Some(LeapRule { every: 4, except: None, also: None, month: 99, extra_days: 1 }),
            ..Calendar::gregorian()
        };
        assert!(matches!(bad.validate(), Err(Error::BadLeapMonth { .. })));
        assert!(Calendar::gregorian().audit_leap_rule());
    }

    #[test]
    fn renders_eras_and_weekdays() {
        let cal = odd_world();
        let day = cal.to_day(CivilDate::ymd(812, 4, 17)).unwrap();
        assert_eq!(cal.format_numeric(day), "0812-04-17");
        assert_eq!(cal.format_long(day), "17 Month4, 813 AR");
        assert!(cal.weekday(day).is_some());
    }
}
