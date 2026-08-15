//! The terrain payload, and the one command that fetches it.
//!
//! Terrain crosses the bridge exactly once per world. It is by far the largest thing the
//! frontend receives — a few thousand polygons — and it is also the only thing that does
//! not change when the scrubber moves, so paying for it once is the whole point of
//! keeping it out of [`crate::commands::snapshot`].
//!
//! Coordinates are rounded on the way out. Four decimals is a fifth of a pixel on a
//! 2,000-pixel raster, and it takes about a third off the wire.

use std::collections::BTreeMap;

use serde::Serialize;
use tauri::State;
use wb_store::World;
use wb_terrain::{Biome, Terrain};

use crate::commands::AppState;

/// Enough precision to be exact at the source raster's resolution, and no more.
fn round(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// A closed loop as `[x0, y0, x1, y1, …]`. Flat because the nesting is half the bytes.
fn flatten(points: &[[f64; 2]]) -> Vec<f64> {
    points.iter().flat_map(|p| [round(p[0]), round(p[1])]).collect()
}

#[derive(Serialize)]
pub struct TerrainDto {
    /// `width / height` of the source raster. The view needs it to keep hatching and
    /// stroke weights from stretching.
    pub aspect: f64,
    pub sea_level: f32,
    /// The traced coastline, largest ring first.
    pub coast: Vec<RingDto>,
    /// One flat loop per cell, index-aligned with every array below it.
    pub cells: Vec<Vec<f64>>,
    pub is_land: Vec<bool>,
    pub lake: Vec<bool>,
    pub height: Vec<f32>,
    pub temperature: Vec<f32>,
    pub precipitation: Vec<f32>,
    /// Index into `palette`.
    pub biome: Vec<u8>,
    pub palette: Vec<BiomeDto>,
    pub rivers: Vec<RiverDto>,
    /// The terrain under every entity that has a marker, keyed by entity id. Computed
    /// here because the join is the same at every instant, and the inspector should not
    /// have to make a second round trip to say what the ground is like.
    pub places: BTreeMap<String, PlaceDto>,
    pub summary: SummaryDto,
}

#[derive(Serialize)]
pub struct RingDto {
    pub points: Vec<f64>,
    pub is_hole: bool,
}

#[derive(Serialize)]
pub struct BiomeDto {
    pub label: &'static str,
    pub color: &'static str,
    pub water: bool,
}

#[derive(Serialize)]
pub struct RiverDto {
    pub points: Vec<f64>,
    /// Accumulated flux at each point, so the channel widens downstream.
    pub flux: Vec<f32>,
    pub order: u32,
    pub mouth: &'static str,
}

#[derive(Serialize)]
pub struct PlaceDto {
    pub biome: &'static str,
    pub color: &'static str,
    /// Height above sea level as a fraction of the land's full range, `0.0` at the shore.
    pub elevation: f32,
    pub temperature: f32,
    pub precipitation: f32,
    pub on_river: bool,
    pub coastal: bool,
}

#[derive(Serialize)]
pub struct SummaryDto {
    pub land_fraction: f64,
    pub cells: usize,
    pub islands: usize,
    pub rivers: usize,
    pub lake_cells: usize,
    pub coast_points: usize,
    pub temperature_min: f32,
    pub temperature_max: f32,
    pub biomes: Vec<(String, usize)>,
}

impl TerrainDto {
    pub fn of(world: &World, t: &Terrain) -> Self {
        let palette: Vec<BiomeDto> = Biome::ALL
            .iter()
            .map(|b| BiomeDto { label: b.label(), color: b.color(), water: b.is_water() })
            .collect();
        let index = |b: Biome| Biome::ALL.iter().position(|x| *x == b).unwrap_or(0) as u8;

        let mut places = BTreeMap::new();
        for entity in world.entities.values() {
            let Some(marker) = entity.marker else { continue };
            let Some(place) = t.describe(marker) else { continue };
            places.insert(entity.id.clone(), PlaceDto::of(t, &place));
        }

        Self {
            aspect: t.aspect,
            sea_level: t.sea_level,
            coast: t
                .coast
                .iter()
                .map(|r| RingDto { points: flatten(&r.points), is_hole: r.is_hole })
                .collect(),
            cells: t.cells.polygons.iter().map(|p| flatten(p)).collect(),
            is_land: t.cells.is_land.clone(),
            lake: t.cells.lake.clone(),
            height: t.cells.height.clone(),
            temperature: t.cells.temperature.clone(),
            precipitation: t.cells.precipitation.clone(),
            biome: t.cells.biome.iter().map(|b| index(*b)).collect(),
            palette,
            rivers: t
                .rivers
                .iter()
                .map(|r| RiverDto {
                    points: flatten(&r.points),
                    flux: r.flux.clone(),
                    order: r.order,
                    mouth: match r.mouth {
                        wb_terrain::Mouth::Sea => "sea",
                        wb_terrain::Mouth::Lake => "lake",
                        wb_terrain::Mouth::Endorheic => "sink",
                    },
                })
                .collect(),
            places,
            summary: SummaryDto {
                land_fraction: t.stats.land_fraction,
                cells: t.stats.cells,
                islands: t.stats.islands,
                rivers: t.stats.rivers,
                lake_cells: t.stats.lake_cells,
                coast_points: t.stats.coast_points,
                temperature_min: t.stats.temperature_min,
                temperature_max: t.stats.temperature_max,
                biomes: t.stats.biomes.iter().map(|b| (b.label.clone(), b.cells)).collect(),
            },
        }
    }
}

impl PlaceDto {
    fn of(t: &Terrain, place: &wb_terrain::Place) -> Self {
        let sea = t.sea_level;
        let ceiling = (t.stats.highest - sea).max(f32::EPSILON);
        Self {
            biome: place.biome.label(),
            color: place.biome.color(),
            elevation: ((place.height - sea) / ceiling).clamp(0.0, 1.0),
            temperature: place.temperature,
            precipitation: place.precipitation,
            on_river: place.on_river,
            coastal: t.cells.neighbors[place.cell]
                .iter()
                .any(|n| t.cells.is_land[*n as usize] != place.is_land),
        }
    }
}

/// The imported raster itself, as a data URL, or `null` if the world declares no map.
///
/// Stage 1 of the pipeline stays on the map as a display layer — the vectors are what is
/// queryable, but the writer's own art is what they recognise. Sent as a data URL rather
/// than through the asset protocol so it needs no filesystem scope: the image is one file
/// the world already told us about, fetched once.
#[tauri::command]
pub fn map_image(state: State<'_, AppState>) -> Result<Option<String>, String> {
    let found = state.read(|world| {
        world.map.as_ref().map(|spec| (world.root.join(&spec.image), spec.image.clone()))
    })?;
    let Some((path, relative)) = found else { return Ok(None) };

    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mime = match relative.extension().and_then(|e| e.to_str()) {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "image/png",
    };
    Ok(Some(format!("data:{mime};base64,{}", base64(&bytes))))
}

/// Standard base64, hand-rolled to keep a dependency out of the shell for twenty lines.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            // A group of three bytes is four sextets; a short final group pads the
            // sextets it has no bits for.
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i)) as usize & 0x3F]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The terrain for the open world, or `null` if it declares no map.
///
/// The first call after an edit to the map or its settings pays for the whole pipeline —
/// about a second. Every call after that reads the cache under `.worldbuilder/`.
#[tauri::command]
pub fn terrain(state: State<'_, AppState>) -> Result<Option<TerrainDto>, String> {
    state.read(|world| match world.terrain() {
        Ok(Some(t)) => Ok(Some(TerrainDto::of(world, &t))),
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    })?
}

/// Just the per-record ground, for after a marker moves.
///
/// The terrain itself is cached and does not change — but `places` is a join of every
/// entity's marker against it, so placing a marker adds an entry. Refetching the whole
/// `terrain` payload to learn that would re-serialize a few thousand cell polygons,
/// rivers and coastline across the bridge to recompute one small map.
#[tauri::command]
pub fn terrain_places(state: State<'_, AppState>) -> Result<BTreeMap<String, PlaceDto>, String> {
    state.read(|world| match world.terrain() {
        Ok(Some(t)) => {
            let mut places = BTreeMap::new();
            for entity in world.entities.values() {
                let Some(marker) = entity.marker else { continue };
                let Some(place) = t.describe(marker) else { continue };
                places.insert(entity.id.clone(), PlaceDto::of(&t, &place));
            }
            Ok(places)
        }
        Ok(None) => Ok(BTreeMap::new()),
        Err(e) => Err(e.to_string()),
    })?
}
