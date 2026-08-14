//! Terrain: the substrate the timeline is projected onto.
//!
//! Everything in `wb-store` is time-indexed — who held the Vale in 812, whether Aldric
//! was alive. Terrain is the opposite: it does not change, which is exactly why it lives
//! in its own crate with no knowledge of worlds, entities or dates.
//!
//! That split buys something concrete. The political layer is refetched whenever the
//! scrubber crosses a change point; terrain is fetched once and never again. And because
//! the pipeline is a pure function of a raster plus a few dozen numbers, its output is a
//! **build product**: cached under `.worldbuilder/`, never committed, and rebuilt from
//! the writer's files whenever the digest moves.
//!
//! The stages are [`mask`] → [`contour`] → [`simplify`] → [`cells`] → [`height`] →
//! [`climate`] → [`rivers`] → [`biome`], and [`build`] is the whole of it.

pub mod biome;
pub mod cells;
pub mod climate;
pub mod contour;
pub mod height;
pub mod mask;
pub mod params;
pub mod rivers;
pub mod rng;
pub mod simplify;

use serde::{Deserialize, Serialize};

pub use biome::Biome;
pub use contour::Ring;
pub use params::{ClimateParams, HeightParams, Range, RiverParams, SeaParams, TerrainParams};
pub use rivers::{Mouth, River};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the map image could not be read: {0}")]
    Image(String),
    #[error(
        "no land was found. The sea colour {0} matched every pixel — pick the colour \
         from the water in your map, or lower the tolerance."
    )]
    AllSea(String),
    #[error(
        "no sea was found. Nothing matched the sea colour {0}, so the whole map is land \
         and there is no coastline to trace."
    )]
    AllLand(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The finished substrate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Terrain {
    /// Fingerprint of the parameters that produced this. The cache key's other half is
    /// the image itself.
    pub digest: u64,
    /// Pixel size of the source raster, and its `width / height`.
    pub source_width: u32,
    pub source_height: u32,
    pub aspect: f64,
    /// The shore, on the `0.0..=1.0` height scale. Carried rather than left implicit
    /// because every consumer needs it and nobody should recover it from the data.
    pub sea_level: f32,
    /// The coastline, simplified. Largest ring first.
    pub coast: Vec<Ring>,
    pub cells: CellField,
    pub rivers: Vec<River>,
    pub stats: Stats,
}

/// Everything known per cell, as parallel arrays. A struct of arrays rather than an array
/// of structs because it serializes to a third of the size and every stage computes one
/// field at a time anyway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellField {
    pub sites: Vec<[f64; 2]>,
    pub polygons: Vec<Vec<[f64; 2]>>,
    pub neighbors: Vec<Vec<u32>>,
    pub is_land: Vec<bool>,
    pub height: Vec<f32>,
    pub temperature: Vec<f32>,
    pub precipitation: Vec<f32>,
    pub flux: Vec<f32>,
    pub lake: Vec<bool>,
    pub downhill: Vec<Option<u32>>,
    pub biome: Vec<Biome>,
}

impl CellField {
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }
}

/// A summary worth showing a human, and worth handing an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub land_fraction: f64,
    pub cells: usize,
    /// Coastline rings that bound land.
    pub islands: usize,
    /// Rings that bound water inside land.
    pub inlets: usize,
    pub coast_points: usize,
    pub rivers: usize,
    pub lake_cells: usize,
    pub highest: f32,
    pub temperature_min: f32,
    pub temperature_max: f32,
    /// Cell counts by biome, commonest first. Only land and lake biomes.
    pub biomes: Vec<BiomeCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeCount {
    pub biome: Biome,
    pub label: String,
    pub cells: usize,
}

impl Terrain {
    /// The cell containing a normalized point, or the nearest one if it falls in a gap.
    ///
    /// This is the join between terrain and canon: given a settlement's `marker`, it
    /// answers what the ground under it is like.
    pub fn cell_at(&self, p: [f64; 2]) -> Option<usize> {
        let mut best = None;
        let mut best_d = f64::INFINITY;
        for (i, s) in self.cells.sites.iter().enumerate() {
            // Squared distance, with the horizontal squash of normalized coords undone.
            let dx = (s[0] - p[0]) * self.aspect;
            let dy = s[1] - p[1];
            let d2 = dx * dx + dy * dy;
            if d2 < best_d {
                best_d = d2;
                best = Some(i);
            }
        }
        best
    }

    /// What the ground is like at a point, in the terms a writer would use.
    pub fn describe(&self, p: [f64; 2]) -> Option<Place> {
        let i = self.cell_at(p)?;
        Some(Place {
            cell: i,
            biome: self.cells.biome[i],
            biome_label: self.cells.biome[i].label().into(),
            height: self.cells.height[i],
            temperature: self.cells.temperature[i],
            precipitation: self.cells.precipitation[i],
            is_land: self.cells.is_land[i],
            on_river: self.rivers.iter().any(|r| r.cells.contains(&(i as u32))),
        })
    }
}

/// The terrain under a single point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Place {
    pub cell: usize,
    pub biome: Biome,
    pub biome_label: String,
    pub height: f32,
    pub temperature: f32,
    pub precipitation: f32,
    pub is_land: bool,
    pub on_river: bool,
}

/// Decode a map image. Kept here so the `image` crate stops at this crate's edge.
pub fn load_image(bytes: &[u8]) -> Result<image::RgbaImage> {
    image::load_from_memory(bytes)
        .map(|img| img.to_rgba8())
        .map_err(|e| Error::Image(e.to_string()))
}

/// Run the whole pipeline.
pub fn build(img: &image::RgbaImage, p: &TerrainParams) -> Result<Terrain> {
    let hex = format!("#{:02X}{:02X}{:02X}", p.sea.color[0], p.sea.color[1], p.sea.color[2]);

    let m = mask::segment(img, &p.sea);
    let land = m.land_fraction();
    if land <= 0.0005 {
        return Err(Error::AllSea(hex));
    }
    if land >= 0.9995 {
        return Err(Error::AllLand(hex));
    }

    let coast = simplify::simplify(contour::trace(&m), p.detail, m.aspect);
    let sub = cells::substrate(p.cells, p.seed, &m);
    let height = height::heights(&sub, &m, &p.height, p.seed);
    let clim = climate::climate(&sub, &height, p.height.sea_level, &p.climate);
    let net = rivers::rivers(&sub, &height, &clim.precipitation, p.height.sea_level, &p.rivers);
    let biomes = biome::classify(
        &clim.temperature,
        &clim.precipitation,
        &height,
        &sub.is_land,
        &net.lake,
        p.height.sea_level,
    );

    let stats = Stats {
        land_fraction: land,
        cells: sub.len(),
        islands: coast.iter().filter(|r| !r.is_hole).count(),
        inlets: coast.iter().filter(|r| r.is_hole).count(),
        coast_points: coast.iter().map(|r| r.points.len()).sum(),
        rivers: net.rivers.len(),
        lake_cells: net.lake.iter().filter(|l| **l).count(),
        highest: height.iter().copied().fold(0.0, f32::max),
        temperature_min: clim.temperature.iter().copied().fold(f32::INFINITY, f32::min),
        temperature_max: clim.temperature.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        biomes: biome_counts(&biomes),
    };

    Ok(Terrain {
        digest: p.digest(),
        source_width: img.width(),
        source_height: img.height(),
        aspect: m.aspect,
        sea_level: p.height.sea_level,
        coast,
        cells: CellField {
            sites: sub.sites,
            polygons: sub.polygons,
            neighbors: sub.neighbors,
            is_land: sub.is_land,
            height,
            temperature: clim.temperature,
            precipitation: clim.precipitation,
            flux: net.flux,
            lake: net.lake,
            downhill: net.downhill,
            biome: biomes,
        },
        rivers: net.rivers,
        stats,
    })
}

fn biome_counts(biomes: &[Biome]) -> Vec<BiomeCount> {
    let mut counts: Vec<BiomeCount> = Biome::ALL
        .iter()
        .map(|b| BiomeCount {
            biome: *b,
            label: b.label().into(),
            cells: biomes.iter().filter(|c| *c == b).count(),
        })
        .filter(|c| c.cells > 0 && !matches!(c.biome, Biome::Ocean | Biome::Shelf))
        .collect();
    counts.sort_by(|a, b| b.cells.cmp(&a.cells).then(a.label.cmp(&b.label)));
    counts
}
