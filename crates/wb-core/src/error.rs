use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("calendar must define at least one month")]
    EmptyCalendar,

    #[error("month {month} does not exist in calendar `{calendar}` ({count} months)")]
    NoSuchMonth { calendar: String, month: u8, count: usize },

    #[error("day {day} does not exist in {month_name} {year} ({len} days)")]
    NoSuchDay { year: i64, month_name: String, day: u16, len: u16 },

    #[error("leap rule targets month {month}, which does not exist ({count} months)")]
    BadLeapMonth { month: usize, count: usize },

    #[error("could not parse date `{input}`: {reason}")]
    BadDate { input: String, reason: String },

    #[error("could not parse duration `{input}`: {reason}")]
    BadDuration { input: String, reason: String },

    /// A relative anchor points at a node that was never defined.
    #[error("`{from}` is anchored to `{missing}`, which does not exist")]
    DanglingAnchor { from: String, missing: String },

    /// Anchors form a loop, so no date can be resolved. Reported as the full ring
    /// so the UI can show the writer exactly which events to break apart.
    #[error("anchor cycle: {}", .path.join(" → "))]
    AnchorCycle { path: Vec<String> },

    #[error("range is inverted: `{lo}` is later than `{hi}`")]
    InvertedRange { lo: String, hi: String },
}
