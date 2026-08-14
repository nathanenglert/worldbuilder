//! Stage 5 — the cell substrate.
//!
//! Poisson-disc sites, Delaunay triangulation, Voronoi cells. Everything downstream —
//! height, climate, rivers, biomes — is computed on this graph rather than on pixels,
//! because a river needs neighbours and a raster only has eight of them.

use serde::{Deserialize, Serialize};

use crate::mask::Mask;

/// The mesh, as a struct of arrays. Every vector has the same length.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Substrate {
    /// `width / height` of the source image.
    pub aspect: f64,
    /// Cell centres, normalized `0..1`.
    pub sites: Vec<[f64; 2]>,
    /// The Voronoi cell around each site, clipped to the unit square, wound consistently.
    pub polygons: Vec<Vec<[f64; 2]>>,
    /// Delaunay adjacency. Symmetric: if `b` is in `neighbors[a]`, `a` is in `neighbors[b]`.
    pub neighbors: Vec<Vec<u32>>,
    /// Sampled from the mask at each site.
    pub is_land: Vec<bool>,
}

impl Substrate {
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// Distance between two sites with the horizontal squash of normalized coordinates
    /// undone. Every metric quantity in the pipeline goes through here.
    pub fn distance(&self, a: usize, b: usize) -> f64 {
        let (p, q) = (self.sites[a], self.sites[b]);
        f64::hypot((p[0] - q[0]) * self.aspect, p[1] - q[1])
    }
}

/// Scatter `target` sites and mesh them.
///
/// Requirements:
///
/// - **Bridson Poisson-disc sampling** over `[0, aspect] × [0, 1]`, then divided back into
///   normalized coordinates. Blue noise, not uniform random: Voronoi cells over uniform
///   random points vary wildly in size, and a river's course would follow the sampling
///   artefacts instead of the terrain. Pick the radius so the count lands near `target`.
/// - Deterministic in `seed`, via [`crate::rng::Rng`]. Same seed, same mesh, forever.
/// - **Delaunay** via the `delaunator` crate, then Voronoi cells from triangle
///   circumcenters.
/// - Hull sites have unbounded Voronoi cells. Handle it by seeding a frame of extra sites
///   outside the unit square and discarding them afterwards, so every returned cell is a
///   closed polygon — no special cases downstream.
/// - Clip every polygon to the unit square (Sutherland–Hodgman). A cell must never poke
///   outside the map.
/// - `neighbors` comes from Delaunay edges among the surviving sites, deduplicated, and
///   must be symmetric.
/// - `is_land` is `mask.sample(site)`.
pub fn substrate(target: usize, seed: u64, mask: &Mask) -> Substrate {
    let _ = (target, seed, mask);
    todo!("stage 5")
}
