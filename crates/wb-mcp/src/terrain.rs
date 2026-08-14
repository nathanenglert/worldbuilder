//! Terrain, as an agent sees it.
//!
//! The map is a projection of the timeline, but the *ground* is not: it does not change
//! when the date does. So these payloads carry no dates at all, and an agent that has
//! asked once need not ask again.
//!
//! What makes them worth having is placement. "Greyford is upriver from Marrow on the
//! Silt" is a sentence in someone's notes; [`SiteOut`] is how an agent turns it into a
//! coordinate it can actually propose, instead of picking one that puts a river town on
//! a ridge.

use schemars::JsonSchema;
use serde::Serialize;
use wb_store::World;
use wb_terrain::Terrain;

/// What the pipeline made of the writer's map image.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TerrainSummary {
    /// The imported raster this was derived from.
    pub source: String,
    pub source_pixels: [u32; 2],
    pub land_fraction: f64,
    /// Substrate cells. Every quantity below is per cell.
    pub cells: usize,
    /// Coastline rings bounding land — the mainland plus every island.
    pub islands: usize,
    pub rivers: usize,
    pub lake_cells: usize,
    pub temperature_c: [f32; 2],
    /// Land and lake cell counts by biome, commonest first.
    pub biomes: Vec<BiomeCount>,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct BiomeCount {
    pub biome: String,
    pub cells: usize,
}

impl TerrainSummary {
    pub fn of(world: &World, t: &Terrain) -> Self {
        let source = world
            .map
            .as_ref()
            .map(|m| m.image.display().to_string())
            .unwrap_or_else(|| "unknown".into());

        Self {
            source,
            source_pixels: [t.source_width, t.source_height],
            land_fraction: t.stats.land_fraction,
            cells: t.stats.cells,
            islands: t.stats.islands,
            rivers: t.stats.rivers,
            lake_cells: t.stats.lake_cells,
            temperature_c: [t.stats.temperature_min, t.stats.temperature_max],
            biomes: t
                .stats
                .biomes
                .iter()
                .map(|b| BiomeCount { biome: b.label.clone(), cells: b.cells })
                .collect(),
            note: "Terrain is derived from the map image and the settings in world.yaml. \
                   It is not canon and cannot be proposed against — to change it, change \
                   the image or the `map.terrain` block. Coordinates are normalized 0..1 \
                   over the image, with y increasing southward.",
        }
    }
}

/// The ground at one point.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PlaceOut {
    pub at: [f64; 2],
    /// The record this was asked about, when it was asked by id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    pub biome: String,
    pub is_land: bool,
    /// Height above sea level as a fraction of the map's full relief. `0.0` is the shore.
    pub elevation: f32,
    pub temperature_c: f32,
    /// Rainfall as a fraction of the wettest cell on the map. Relative, not millimetres —
    /// the settings are a writer's dials, not a rain gauge.
    pub rainfall: f32,
    pub on_river: bool,
    pub coastal: bool,
    /// Named records nearby, nearest first, with the distance as a fraction of the map's
    /// width. Included so an answer can be checked against something a reader knows.
    pub near: Vec<Nearby>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Nearby {
    pub id: String,
    pub name: String,
    pub distance: f64,
}

impl PlaceOut {
    pub fn of(world: &World, t: &Terrain, at: [f64; 2], entity: Option<String>) -> Option<Self> {
        let place = t.describe(at)?;
        Some(Self {
            at: [round(at[0]), round(at[1])],
            entity,
            biome: place.biome.label().into(),
            is_land: place.is_land,
            elevation: relief(t, place.height),
            temperature_c: (place.temperature * 10.0).round() / 10.0,
            rainfall: (place.precipitation * 100.0).round() / 100.0,
            on_river: place.on_river,
            coastal: t.cells.neighbors[place.cell]
                .iter()
                .any(|n| t.cells.is_land[*n as usize] != place.is_land),
            near: nearby(world, t, at, 3),
        })
    }
}

/// A candidate location, and why it qualified.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SiteOut {
    pub at: [f64; 2],
    /// How far this is from the record given as `near`, as a fraction of the map's width.
    /// Absent when no anchor was given. Candidates come back in this order, so without it
    /// an agent would have to take the ranking on trust.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_anchor: Option<f64>,
    pub biome: String,
    pub elevation: f32,
    pub temperature_c: f32,
    pub rainfall: f32,
    pub on_river: bool,
    pub coastal: bool,
    pub near: Vec<Nearby>,
}

/// Distance between two normalized points with the map's horizontal squash undone, so it
/// is a fraction of the map's *width* rather than of whichever axis happened to be longer.
pub fn distance(t: &Terrain, a: [f64; 2], b: [f64; 2]) -> f64 {
    f64::hypot((a[0] - b[0]) * t.aspect, a[1] - b[1]) / t.aspect.max(f64::MIN_POSITIVE)
}

pub fn nearby(world: &World, t: &Terrain, at: [f64; 2], limit: usize) -> Vec<Nearby> {
    let mut found: Vec<Nearby> = world
        .entities
        .values()
        .filter_map(|e| {
            let marker = e.marker?;
            Some(Nearby {
                id: e.id.clone(),
                name: e.name.clone(),
                distance: (distance(t, at, marker) * 1000.0).round() / 1000.0,
            })
        })
        .collect();
    found.sort_by(|a, b| a.distance.total_cmp(&b.distance).then(a.id.cmp(&b.id)));
    found.truncate(limit);
    found
}

/// Height as a fraction of the land's relief, `0.0` at the shore. Absolute heights are on
/// an arbitrary `0..1` scale nobody authored, so reporting them would invite an agent to
/// treat them as metres.
pub fn relief(t: &Terrain, height: f32) -> f32 {
    let ceiling = (t.stats.highest - t.sea_level).max(f32::EPSILON);
    let v = ((height - t.sea_level) / ceiling).clamp(0.0, 1.0);
    (v * 100.0).round() / 100.0
}

fn round(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
