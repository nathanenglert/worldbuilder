//! The date notation writers actually type.
//!
//! ```text
//!   812                  a whole year
//!   0812-04              a month
//!   0812-04-17           a day
//!   812~                 approximately
//!   810..815             somewhere in range
//!   >812  /  <812        after / before
//!   @evt_sundering+40y   forty years after the Sundering
//!   @act_aldric.death-2g two generations before Aldric died
//!   ?                    genuinely unplaced
//! ```

use crate::calendar::{CivilDate, Duration};
use crate::date::DateExpr;
use crate::error::{Error, Result};

fn bad(input: &str, reason: impl Into<String>) -> Error {
    Error::BadDate { input: input.to_string(), reason: reason.into() }
}

pub fn parse_date(input: &str) -> Result<DateExpr> {
    let s = input.trim();
    if s.is_empty() {
        return Err(bad(input, "empty"));
    }
    if s == "?" {
        return Ok(DateExpr::Unknown);
    }
    if let Some(rest) = s.strip_prefix('@') {
        return parse_anchor(input, rest.trim());
    }
    if let Some(rest) = s.strip_prefix('>') {
        return Ok(DateExpr::After { date: parse_civil(input, rest.trim())?.0 });
    }
    if let Some(rest) = s.strip_prefix('<') {
        return Ok(DateExpr::Before { date: parse_civil(input, rest.trim())?.0 });
    }
    if let Some((lo, hi)) = s.split_once("..") {
        return Ok(DateExpr::Range {
            lo: parse_civil(input, lo.trim())?.0,
            hi: parse_civil(input, hi.trim())?.0,
        });
    }
    let (date, approx) = parse_civil(input, s)?;
    Ok(DateExpr::Civil { date, approx })
}

/// Parse a civil date, returning it alongside whether it carried a `~`.
pub fn parse_civil(input: &str, s: &str) -> Result<(CivilDate, bool)> {
    let approx = s.ends_with('~');
    let s = if approx { &s[..s.len() - 1] } else { s };

    let negative = s.starts_with('-');
    let body = if negative { &s[1..] } else { s };
    if body.is_empty() {
        return Err(bad(input, "no year"));
    }

    let parts: Vec<&str> = body.split('-').collect();
    if parts.len() > 3 {
        return Err(bad(input, "too many components; expected year[-month[-day]]"));
    }
    for p in &parts {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad(input, format!("`{p}` is not a number")));
        }
    }

    let year: i64 = parts[0].parse().map_err(|_| bad(input, "year out of range"))?;
    let year = if negative { -year } else { year };

    let month = match parts.get(1) {
        Some(p) => Some(p.parse::<u8>().map_err(|_| bad(input, "month out of range"))?),
        None => None,
    };
    let day = match parts.get(2) {
        Some(p) => Some(p.parse::<u16>().map_err(|_| bad(input, "day out of range"))?),
        None => None,
    };

    Ok((CivilDate { year, month, day }, approx))
}

fn parse_anchor(input: &str, rest: &str) -> Result<DateExpr> {
    let approx = rest.ends_with('~');
    let rest = if approx { &rest[..rest.len() - 1] } else { rest };

    let split = rest.find(['+', '-']);
    let (node, offset) = match split {
        Some(0) => return Err(bad(input, "anchor has no node id")),
        Some(i) => {
            let sign = rest.as_bytes()[i];
            let dur = parse_duration(&rest[i + 1..])?;
            (&rest[..i], if sign == b'-' { dur.negated() } else { dur })
        }
        None => (rest, Duration::default()),
    };

    if node.is_empty() {
        return Err(bad(input, "anchor has no node id"));
    }
    if !node.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.') {
        return Err(bad(input, "node ids may contain letters, digits, `_` and `.` only"));
    }

    Ok(DateExpr::Anchor { node: node.to_string(), offset, approx })
}

/// `40y`, `1y6m`, `12d`, `2g`. Units are years, months, days, generations.
pub fn parse_duration(input: &str) -> Result<Duration> {
    let s = input.trim();
    if s.is_empty() {
        return Err(Error::BadDuration { input: input.into(), reason: "empty".into() });
    }

    let mut dur = Duration::default();
    let mut digits = String::new();
    let mut saw_unit = false;

    for ch in s.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        if digits.is_empty() {
            return Err(Error::BadDuration {
                input: input.into(),
                reason: format!("`{ch}` has no number before it"),
            });
        }
        let n: i64 = digits.parse().map_err(|_| Error::BadDuration {
            input: input.into(),
            reason: "number out of range".into(),
        })?;
        digits.clear();
        saw_unit = true;
        match ch {
            'y' => dur.years += n,
            'm' => dur.months += n,
            'd' => dur.days += n,
            'g' => dur.generations += n,
            _ => {
                return Err(Error::BadDuration {
                    input: input.into(),
                    reason: format!("unknown unit `{ch}`; expected y, m, d or g"),
                });
            }
        }
    }

    if !digits.is_empty() {
        return Err(Error::BadDuration {
            input: input.into(),
            reason: format!("`{digits}` has no unit"),
        });
    }
    if !saw_unit {
        return Err(Error::BadDuration { input: input.into(), reason: "no units".into() });
    }
    Ok(dur)
}

impl std::fmt::Display for DateExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Civil { date, approx } => {
                write!(f, "{date}{}", if *approx { "~" } else { "" })
            }
            Self::Range { lo, hi } => write!(f, "{lo}..{hi}"),
            Self::After { date } => write!(f, ">{date}"),
            Self::Before { date } => write!(f, "<{date}"),
            Self::Anchor { node, offset, approx } => {
                write!(f, "@{node}")?;
                if !offset.is_zero() {
                    // Durations print their own sign, so only `+` needs adding.
                    let negative = offset.years < 0
                        || offset.months < 0
                        || offset.days < 0
                        || offset.generations < 0;
                    if negative {
                        write!(f, "-{}", offset.negated())?;
                    } else {
                        write!(f, "+{offset}")?;
                    }
                }
                write!(f, "{}", if *approx { "~" } else { "" })
            }
            Self::Unknown => write!(f, "?"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_precision() {
        assert_eq!(
            parse_date("812").unwrap(),
            DateExpr::Civil { date: CivilDate::year(812), approx: false }
        );
        assert_eq!(
            parse_date("0812-04").unwrap(),
            DateExpr::Civil { date: CivilDate::ym(812, 4), approx: false }
        );
        assert_eq!(
            parse_date("0812-04-17").unwrap(),
            DateExpr::Civil { date: CivilDate::ymd(812, 4, 17), approx: false }
        );
    }

    #[test]
    fn parses_prehistory() {
        let DateExpr::Civil { date, .. } = parse_date("-1200-03").unwrap() else {
            panic!("expected a civil date")
        };
        assert_eq!(date, CivilDate::ym(-1200, 3));
    }

    #[test]
    fn parses_vagueness_markers() {
        assert_eq!(
            parse_date("812~").unwrap(),
            DateExpr::Civil { date: CivilDate::year(812), approx: true }
        );
        assert_eq!(
            parse_date("810..815").unwrap(),
            DateExpr::Range { lo: CivilDate::year(810), hi: CivilDate::year(815) }
        );
        assert_eq!(parse_date(">812").unwrap(), DateExpr::After { date: CivilDate::year(812) });
        assert_eq!(parse_date("<812").unwrap(), DateExpr::Before { date: CivilDate::year(812) });
        assert_eq!(parse_date("?").unwrap(), DateExpr::Unknown);
    }

    #[test]
    fn parses_anchors_with_and_without_offsets() {
        assert_eq!(
            parse_date("@evt_sundering").unwrap(),
            DateExpr::Anchor {
                node: "evt_sundering".into(),
                offset: Duration::default(),
                approx: false
            }
        );
        assert_eq!(
            parse_date("@evt_sundering+40y").unwrap(),
            DateExpr::Anchor {
                node: "evt_sundering".into(),
                offset: Duration::years(40),
                approx: false
            }
        );
        assert_eq!(
            parse_date("@act_aldric.death-2g~").unwrap(),
            DateExpr::Anchor {
                node: "act_aldric.death".into(),
                offset: Duration::generations(-2),
                approx: true
            }
        );
    }

    #[test]
    fn parses_compound_durations() {
        assert_eq!(
            parse_duration("1y6m12d").unwrap(),
            Duration { years: 1, months: 6, days: 12, generations: 0 }
        );
    }

    #[test]
    fn rejects_malformed_input() {
        for input in ["", "  ", "81a", "812-", "812-04-17-99", "@", "@evt+", "@evt+5", "@evt+5z"] {
            assert!(parse_date(input).is_err(), "`{input}` should not parse");
        }
        assert!(parse_date("@bad-id+1y").is_err(), "hyphens are the offset separator");
    }

    #[test]
    fn every_form_round_trips_through_display() {
        for input in [
            "0812",
            "0812-04",
            "0812-04-17",
            "0812~",
            "0810..0815",
            ">0812",
            "<0812",
            "-1200-03",
            "@evt_sundering",
            "@evt_sundering+40y",
            "@act_aldric.death-2g~",
            "?",
        ] {
            let parsed = parse_date(input).unwrap();
            assert_eq!(parsed.to_string(), input, "round-trip of `{input}`");
            assert_eq!(parse_date(&parsed.to_string()).unwrap(), parsed);
        }
    }
}
