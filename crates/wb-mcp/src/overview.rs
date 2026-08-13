//! Orientation: what an agent needs before it asks a single question.
//!
//! This is the highest-leverage payload in the server. An agent that does not know the
//! months are thirty days long, that `~` widens a year by two, or that this world
//! already calls a ruler `owner` will produce dates that resolve wrong and facts that
//! duplicate an existing vocabulary under a new name. None of that is recoverable
//! downstream, so it is all stated up front.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Serialize;
use wb_store::World;

use crate::dto::primitive_name;

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MonthDto {
    pub number: u8,
    pub name: String,
    pub days: u16,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EraDto {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub abbr: String,
    pub starts: i64,
    pub descending: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CalendarDto {
    pub name: String,
    pub days_in_year: i64,
    pub months: Vec<MonthDto>,
    pub weekdays: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub eras: Vec<EraDto>,
    /// Years per generation, used by `g` offsets and the parentage rule.
    pub generation_years: i64,
    pub has_leap_rule: bool,
}

/// How far a trailing `~` widens a date on each side, in days.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FuzzDto {
    pub written_to_the_year: i64,
    pub written_to_the_month: i64,
    pub written_to_the_day: i64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SyntaxDto {
    pub form: &'static str,
    pub means: &'static str,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TypeDto {
    pub name: String,
    /// Which of the five engine roles this type behaves like.
    pub primitive: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub count: usize,
}

/// An attribute already in use, so an agent extends the world's vocabulary instead of
/// inventing a parallel one. A world with `owner` does not want `ruled_by` as well.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AttrDto {
    pub attr: String,
    pub used: usize,
    /// Whether several values may hold at once. Anything else is single-valued, and two
    /// overlapping assertions of it are reported as a contradiction.
    pub multi_valued: bool,
    /// A few real values, to show the shape expected.
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SpanSummary {
    pub earliest_day: i64,
    pub earliest_label: String,
    pub latest_day: i64,
    pub latest_label: String,
    /// Instants where anything in the world could change. Nothing between two adjacent
    /// points can differ, so these are the only dates worth sampling.
    pub change_points: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WorldOverview {
    pub name: String,
    pub root: String,
    pub entities: usize,
    pub events: usize,
    pub pending_proposals: usize,
    /// `(definite, possible)` consistency findings as things stand.
    pub definite_findings: usize,
    pub possible_findings: usize,
    pub calendar: CalendarDto,
    pub fuzz: FuzzDto,
    pub date_syntax: Vec<SyntaxDto>,
    pub types: Vec<TypeDto>,
    /// Types used by a record but never declared in `world.yaml` — nearly always typos.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub undeclared_types: Vec<String>,
    pub attributes: Vec<AttrDto>,
    pub span: Option<SpanSummary>,
    pub gestation_days: i64,
    /// How many times the world was reloaded because its files changed under the server.
    pub reloads: u64,
}

pub const DATE_SYNTAX: &[SyntaxDto] = &[
    SyntaxDto { form: "0812", means: "that year, and precise only to the year" },
    SyntaxDto { form: "0812-04", means: "that month" },
    SyntaxDto { form: "0812-04-17", means: "that day" },
    SyntaxDto { form: "-1200-03", means: "a negative year, for prehistory" },
    SyntaxDto { form: "0812~", means: "about then; widened by this world's fuzz" },
    SyntaxDto { form: "0810..0815", means: "somewhere in that range" },
    SyntaxDto { form: ">0812", means: "after that, end unknown" },
    SyntaxDto { form: "<0812", means: "before that, start unknown" },
    SyntaxDto { form: "@evt_siege_of_marrow", means: "whenever that event resolves to" },
    SyntaxDto {
        form: "@evt_siege_of_marrow+1y",
        means: "offset from an anchor; units y, m, d, g (generations), combinable as 1y6m",
    },
    SyntaxDto {
        form: "@act_aldric_vane.death",
        means: "an entity's lifespan end; .birth/.death and .start/.end both resolve",
    },
    SyntaxDto { form: "?", means: "genuinely unknown — a valid answer, not a placeholder" },
];

impl WorldOverview {
    pub fn of(world: &World, pending: usize, reloads: u64) -> Self {
        let cal = &world.calendar;
        let report = wb_check::check(world);
        let (definite, possible) = report.counts();

        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for e in world.entities.values() {
            *counts.entry(e.type_name.as_str()).or_default() += 1;
        }

        let types = world
            .types
            .values()
            .map(|t| TypeDto {
                name: t.name.clone(),
                primitive: primitive_name(t.primitive),
                description: t.description.clone(),
                count: counts.get(t.name.as_str()).copied().unwrap_or(0),
            })
            .collect();

        let mut attrs: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for e in world.entities.values() {
            for f in &e.facts {
                attrs.entry(f.attr.as_str()).or_default().push(f.value.to_string());
            }
        }
        let attributes = attrs
            .into_iter()
            .map(|(attr, mut values)| {
                let used = values.len();
                values.sort();
                values.dedup();
                values.truncate(3);
                AttrDto {
                    attr: attr.to_string(),
                    used,
                    multi_valued: world.rules.is_multi_valued(attr),
                    examples: values,
                }
            })
            .collect();

        let points = world.change_points();
        let span = match (points.first(), points.last()) {
            (Some(&lo), Some(&hi)) => Some(SpanSummary {
                earliest_day: lo.0,
                earliest_label: cal.format_long(lo),
                latest_day: hi.0,
                latest_label: cal.format_long(hi),
                change_points: points.iter().map(|d| d.0).collect(),
            }),
            _ => None,
        };

        Self {
            name: world.name.clone(),
            root: world.root.display().to_string(),
            entities: world.entities.len(),
            events: world.events.len(),
            pending_proposals: pending,
            definite_findings: definite,
            possible_findings: possible,
            calendar: CalendarDto {
                name: cal.name.clone(),
                days_in_year: cal.days_in_year(0),
                months: cal
                    .months
                    .iter()
                    .enumerate()
                    .map(|(i, m)| MonthDto {
                        number: (i + 1) as u8,
                        name: m.name.clone(),
                        days: m.days,
                    })
                    .collect(),
                weekdays: cal.weekdays.clone(),
                eras: cal
                    .eras
                    .iter()
                    .map(|e| EraDto {
                        name: e.name.clone(),
                        abbr: e.abbr.clone(),
                        starts: e.starts,
                        descending: e.descending,
                    })
                    .collect(),
                generation_years: cal.generation_years,
                has_leap_rule: cal.leap.is_some(),
            },
            fuzz: FuzzDto {
                written_to_the_year: world.fuzz.year,
                written_to_the_month: world.fuzz.month,
                written_to_the_day: world.fuzz.day,
            },
            date_syntax: DATE_SYNTAX.to_vec(),
            types,
            undeclared_types: world.undeclared_types().keys().cloned().collect(),
            attributes,
            span,
            gestation_days: world.rules.gestation_days,
            reloads,
        }
    }
}
