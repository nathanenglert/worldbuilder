//! Stage 2 — land/sea segmentation.
//!
//! The writer picks the sea colour; everything within `tolerance` of it is water. That
//! is wrong somewhere on every real map, which is why the mask is a *stage* and not a
//! truth: it feeds the contour tracer, and the contour is what gets edited.

use serde::{Deserialize, Serialize};

use crate::params::SeaParams;

/// A land/sea bitmap at the source raster's resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mask {
    pub w: u32,
    pub h: u32,
    /// `width / height` of the source image. Normalized coordinates squash a non-square
    /// map, so every *distance* has to undo that.
    pub aspect: f64,
    /// Row-major, `w * h` entries. `true` is land.
    pub land: Vec<bool>,
}

impl Mask {
    /// Off the edge of the image is sea. A continent that runs off the map would
    /// otherwise trace a coastline along the border, which is a lie about the world.
    pub fn at(&self, x: i64, y: i64) -> bool {
        if x < 0 || y < 0 || x >= i64::from(self.w) || y >= i64::from(self.h) {
            return false;
        }
        self.land[y as usize * self.w as usize + x as usize]
    }

    /// Nearest-neighbour lookup in normalized `0..1` coordinates.
    pub fn sample(&self, p: [f64; 2]) -> bool {
        self.at((p[0] * f64::from(self.w)) as i64, (p[1] * f64::from(self.h)) as i64)
    }

    pub fn land_fraction(&self) -> f64 {
        if self.land.is_empty() {
            return 0.0;
        }
        self.land.iter().filter(|l| **l).count() as f64 / self.land.len() as f64
    }
}

/// Classify every pixel as land or sea.
///
/// The steps, in order:
///
/// 1. A pixel is *water* when its RGB distance to `p.color` is within `p.tolerance` of
///    the colour cube's diagonal. Fully transparent pixels are water too — a map exported
///    with a cut-out sea is common.
/// 2. If `p.flood_from_edge`, only water connected to the image border stays sea.
///    Enclosed water reverts to land here and is recovered as a lake in [`crate::rivers`],
///    which is what an inland sea actually is.
/// 3. Connected blobs of either kind smaller than `p.min_blob_px` are flipped. Antialiased
///    coastlines and map lettering produce hundreds of these, and each one would otherwise
///    become an island.
pub fn segment(img: &image::RgbaImage, p: &SeaParams) -> Mask {
    let _ = (img, p);
    todo!("stage 2")
}
