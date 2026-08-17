//! Lineage and dynasty, for the chart.
//!
//! One payload, because the two things the view draws are the same two things: a row is
//! a record with a lifespan, and a band on that row is a stretch it held something for.
//! That is why there is no family-tree type here and none in `wb-store` either — DESIGN.md
//! §3.1 says a bloodline is parentage edges over existence intervals, and this is what it
//! looks like when nothing more is invented.
//!
//! Every span crosses as **four** numbers rather than two. `earliest`/`latest` is the
//! possible window and `from`/`to` is the certain core, so the chart can feather the ends
//! of a life that begins `0749~` instead of drawing a hard edge on a date the world has
//! openly guessed at.

use serde::Serialize;
use tauri::State;
use wb_core::FuzzyInterval;
use wb_store::World;
use wb_store::kin::{self, Succession, Tenure};

use crate::commands::{AppState, primitive_name};

/// A record with a lifespan: one row of the chart.
#[derive(Serialize)]
pub struct LifeDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub primitive: Option<&'static str>,
    /// Steps down from the oldest recorded forebear. Zero is nobody's child.
    pub generation: usize,
    pub parents: Vec<String>,
    /// The certain core of the lifespan.
    pub from: Option<i64>,
    pub to: Option<i64>,
    /// The possible window around it. Equal to the core when the dates are exact.
    pub earliest: Option<i64>,
    pub latest: Option<i64>,
    pub label: String,
}

/// One record's turn holding something.
#[derive(Serialize)]
pub struct TenureDto {
    pub holder: String,
    pub name: String,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub earliest: Option<i64>,
    pub latest: Option<i64>,
}

#[derive(Serialize)]
pub struct SuccessionDto {
    /// Stable across reloads, so the selected succession survives a save.
    pub key: String,
    pub label: String,
    pub attr: String,
    /// `title` — one value, many holders. `office` — one record, many values.
    pub kind: &'static str,
    pub holders: Vec<TenureDto>,
    /// `[from, to]` stretches nobody held it. Rendered in the warning colour, because a
    /// throne with nobody on it is the most structurally interesting thing in a world.
    pub gaps: Vec<[i64; 2]>,
    /// Stretches two records held it at once. Not an error — a contested claim.
    pub overlaps: Vec<[i64; 2]>,
}

#[derive(Serialize)]
pub struct LineageDto {
    pub lives: Vec<LifeDto>,
    pub successions: Vec<SuccessionDto>,
}

/// A row's label, in the record's own words.
///
/// The bars are drawn from the possible envelope, which is right — a life that begins
/// `0602~` should feather. The *label* must not: the envelope's near edge is `0599-12-21`,
/// a date to the day that nobody wrote and that is not even approximately what the record
/// says. `phrasing` is the one place that knows the difference, and the exported bible
/// learned it first.
fn label_of(world: &World, entity: &wb_store::Entity) -> String {
    let Some(span) = &entity.existence else { return "undated".to_string() };
    let start = wb_store::phrasing::phrase(world, &entity.id, &span.from);
    let end = wb_store::phrasing::phrase(world, &entity.id, &span.to);
    match (start, end) {
        (Some(a), Some(b)) => format!("{a} → {b}"),
        (Some(a), None) => format!("from {a}"),
        (None, Some(b)) => format!("until {b}"),
        (None, None) => "undated".to_string(),
    }
}

fn ends(span: Option<&FuzzyInterval>) -> (Option<i64>, Option<i64>, Option<i64>, Option<i64>) {
    match span {
        None => (None, None, None, None),
        Some(f) => (
            f.certain.from.map(|d| d.0),
            f.certain.to.map(|d| d.0),
            f.possible.from.map(|d| d.0),
            f.possible.to.map(|d| d.0),
        ),
    }
}

fn tenure(world: &World, t: &Tenure<'_>) -> TenureDto {
    let (from, to, earliest, latest) = ends(Some(&t.span));
    TenureDto {
        holder: t.holder.id.clone(),
        name: world.entities.get(&t.holder.id).map_or(t.holder.id.clone(), |e| e.name.clone()),
        from,
        to,
        earliest,
        latest,
    }
}

fn succession(world: &World, s: &Succession<'_>) -> SuccessionDto {
    SuccessionDto {
        key: format!("{}|{}|{}", s.kind.slug(), s.attr, s.of),
        label: s.label.clone(),
        attr: s.attr.clone(),
        kind: s.kind.slug(),
        holders: s.holders.iter().map(|t| tenure(world, t)).collect(),
        gaps: s.gaps.iter().filter_map(|g| Some([g.from?.0, g.to?.0])).collect(),
        overlaps: s.overlaps.iter().filter_map(|o| Some([o.from?.0, o.to?.0])).collect(),
    }
}

/// Rows and batons.
///
/// The rows are not "every record": a chart of four thousand mountains and languages is
/// not a lineage. They are the records descent or succession actually touches — anyone
/// with a parent, anyone who is a parent, every actor, and every holder of anything that
/// ever changed hands.
#[tauri::command]
pub fn lineage(state: State<'_, AppState>) -> Result<LineageDto, String> {
    state.read(|world| {
        let successions = kin::successions(world);

        let mut wanted: std::collections::BTreeSet<String> = successions
            .iter()
            .flat_map(|s| s.holders.iter().map(|t| t.holder.id.clone()))
            .collect();
        for entity in world.entities.values() {
            let is_actor = world.primitive_of(entity) == Some(wb_store::Primitive::Actor);
            if is_actor || !entity.parents.is_empty() {
                wanted.insert(entity.id.clone());
            }
            for parent in &entity.parents {
                if world.entities.contains_key(parent) {
                    wanted.insert(parent.clone());
                }
            }
        }

        let mut lives: Vec<LifeDto> = wanted
            .iter()
            .filter_map(|id| world.entities.get(id))
            .map(|entity| {
                let (from, to, earliest, latest) = ends(world.lifespan(&entity.id));
                LifeDto {
                    id: entity.id.clone(),
                    name: entity.name.clone(),
                    type_name: entity.type_name.clone(),
                    primitive: world.primitive_of(entity).map(primitive_name),
                    generation: kin::generation_of(world, &entity.id),
                    parents: entity
                        .parents
                        .iter()
                        .filter(|p| world.entities.contains_key(*p))
                        .cloned()
                        .collect(),
                    label: label_of(world, entity),
                    from,
                    to,
                    earliest,
                    latest,
                }
            })
            .collect();

        // Generation, then when they started. A chart sorted by id reads as a list.
        lives.sort_by_key(|l| (l.generation, l.earliest.unwrap_or(i64::MAX), l.name.clone()));

        LineageDto {
            successions: successions.iter().map(|s| succession(world, s)).collect(),
            lives,
        }
    })
}
