//! **wb-core** — the temporal engine behind Worldbuilder.
//!
//! The whole product rests on one idea: the map is a projection of the timeline. To
//! project anything, the engine has to answer *what was true at time T* — which means
//! facts carry validity intervals, dates are allowed to be vague, and "40 years after
//! the Sundering" has to survive someone moving the Sundering.
//!
//! Four pieces, in dependency order:
//!
//! - [`calendar`] — arbitrary calendars, and the canonical [`Day`] scalar everything
//!   reduces to, so renaming a month can never corrupt a stored date.
//! - [`date`] — [`DateExpr`], a date as authored: exact, approximate, ranged,
//!   open-ended, relative, or frankly unknown.
//! - [`resolve`] — resolves a world's dates together, walking the anchor graph and
//!   reporting cycles as the full ring.
//! - [`interval`] — validity intervals, Allen's thirteen relations, and the
//!   certain/possible split that lets the map draw an uncertain border as a dashed one.
//!
//! ```
//! use wb_core::{Calendar, Resolver, parse_date};
//! use std::collections::BTreeMap;
//!
//! let calendar = Calendar::gregorian();
//! let dates: BTreeMap<String, _> = [
//!     ("evt_sundering", "0500"),           // sometime that year
//!     ("evt_siege",     "@evt_sundering+312y"),
//! ]
//! .iter()
//! .map(|(id, src)| (id.to_string(), parse_date(src).unwrap()))
//! .collect();
//!
//! let resolved = Resolver::new(&calendar).resolve_all(&dates).unwrap();
//! let siege = resolved["evt_siege"];
//!
//! assert_eq!(calendar.from_day(siege.nominal.unwrap()).year, 812);
//! assert!(!siege.is_exact()); // still known only to the year, as the Sundering was
//! ```

pub mod calendar;
pub mod date;
pub mod error;
pub mod interval;
pub mod parse;
pub mod resolve;

pub use calendar::{Calendar, CivilDate, Day, Duration, Era, LeapRule, Month, Precision};
pub use date::{DateExpr, Fuzz, Resolved};
pub use error::{Error, Result};
pub use interval::{AllenRelation, Containment, FuzzyInterval, Interval, change_points};
pub use parse::{parse_date, parse_duration};
pub use resolve::{NodeMap, Resolver};
