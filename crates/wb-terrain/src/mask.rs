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
/// 3. Connected blobs of either kind smaller than `p.min_blob_px` are flipped, land
///    first. Antialiased coastlines and map lettering produce hundreds of these, and each
///    one would otherwise become an island. A blob of sea touching the image border is
///    never flipped, however small: the open water in the corner of a mostly-land map is
///    not a speck.
///
/// Connectivity is 4-way throughout, for water and for land alike, so the two agree about
/// what a diagonal touch means and the tracer downstream sees no ambiguity.
pub fn segment(img: &image::RgbaImage, p: &SeaParams) -> Mask {
    let (w, h) = img.dimensions();
    let (wu, hu) = (w as usize, h as usize);
    // 0/0 is NaN, and a NaN aspect would poison every distance computed downstream.
    let aspect = if w == 0 || h == 0 { 1.0 } else { f64::from(w) / f64::from(h) };

    let mut land = vec![false; wu * hu];
    if land.is_empty() {
        return Mask { w, h, aspect, land };
    }

    // `tolerance` is a fraction of the colour cube's diagonal, sqrt(3) * 255. Squaring
    // both sides keeps the per-pixel test off the square root.
    let tol = if p.tolerance.is_nan() { 0.0 } else { p.tolerance.clamp(0.0, 1.0) };
    let limit = tol * tol * 3.0 * 255.0 * 255.0;

    for (i, px) in img.pixels().enumerate() {
        let [r, g, b, a] = px.0;
        let dr = i32::from(r) - i32::from(p.color[0]);
        let dg = i32::from(g) - i32::from(p.color[1]);
        let db = i32::from(b) - i32::from(p.color[2]);
        let water = a < OPAQUE_ENOUGH || f64::from(dr * dr + dg * dg + db * db) <= limit;
        land[i] = !water;
    }

    if p.flood_from_edge {
        sink_enclosed_water(&mut land, wu, hu);
    }

    // Nothing has fewer than one pixel, so smaller thresholds are two wasted passes.
    if p.min_blob_px > 1 {
        // Land first: eating a speck of land merges the water either side of it, so a
        // strait one speck wide is not then mistaken for two seas too small to keep.
        despeckle(&mut land, wu, hu, true, p.min_blob_px, false);
        despeckle(&mut land, wu, hu, false, p.min_blob_px, true);
    }

    Mask { w, h, aspect, land }
}

/// Below this, a pixel is a cut-out whatever colour sits underneath it.
const OPAQUE_ENOUGH: u8 = 8;

/// Turn water the border cannot reach into land.
///
/// An enclosed sea is a lake, and [`crate::rivers`] is where lakes are found. Calling it
/// ocean here would cut a second coastline around it and hand the writer an island that
/// is really a shoreline.
fn sink_enclosed_water(land: &mut [bool], w: usize, h: usize) {
    let mut ocean = vec![false; land.len()];
    let mut stack: Vec<usize> = Vec::new();

    let border = (0..w)
        .flat_map(|x| [x, (h - 1) * w + x])
        .chain((0..h).flat_map(|y| [y * w, y * w + w - 1]));
    for i in border {
        if !land[i] && !ocean[i] {
            ocean[i] = true;
            stack.push(i);
        }
    }
    while let Some(i) = stack.pop() {
        for j in neighbors(i, w, h).into_iter().flatten() {
            if !land[j] && !ocean[j] {
                ocean[j] = true;
                stack.push(j);
            }
        }
    }

    for (l, reached) in land.iter_mut().zip(ocean) {
        *l |= !reached;
    }
}

/// Flip every 4-connected component of `kind` holding fewer than `min_px` pixels.
///
/// `spare_border` exempts components touching the image edge, which is how a narrow strip
/// of open sea along the margin survives a threshold sized for specks.
fn despeckle(land: &mut [bool], w: usize, h: usize, kind: bool, min_px: u32, spare_border: bool) {
    let min_px = min_px as usize;
    let mut seen = vec![false; land.len()];
    let mut blob: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    for start in 0..land.len() {
        if seen[start] || land[start] != kind {
            continue;
        }
        blob.clear();
        seen[start] = true;
        stack.push(start);

        let mut size = 0usize;
        let mut on_border = false;
        while let Some(i) = stack.pop() {
            size += 1;
            // Once the component is big enough to keep, its members are never needed.
            if size <= min_px {
                blob.push(i);
            }
            let (x, y) = (i % w, i / w);
            on_border |= x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            for j in neighbors(i, w, h).into_iter().flatten() {
                if !seen[j] && land[j] == kind {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }

        if size < min_px && !(spare_border && on_border) {
            for &i in &blob {
                land[i] = !kind;
            }
        }
    }
}

/// The 4-connected neighbours of a row-major index.
fn neighbors(i: usize, w: usize, h: usize) -> [Option<usize>; 4] {
    let (x, y) = (i % w, i / w);
    [
        (x > 0).then(|| i - 1),
        (x + 1 < w).then(|| i + 1),
        (y > 0).then(|| i - w),
        (y + 1 < h).then(|| i + w),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEA: [u8; 4] = [0x1E, 0x3A, 0x4C, 0xFF];
    const GRASS: [u8; 4] = [0x6B, 0x8E, 0x23, 0xFF];

    /// Both cleanup stages off, so each test opts into the one it is about.
    fn plain() -> SeaParams {
        SeaParams { flood_from_edge: false, min_blob_px: 0, ..SeaParams::default() }
    }

    fn paint(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> image::RgbaImage {
        let mut px = Vec::with_capacity(w as usize * h as usize * 4);
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&f(x, y));
            }
        }
        image::RgbaImage::from_raw(w, h, px).expect("the buffer matches the dimensions")
    }

    fn solid(w: u32, h: u32, c: [u8; 4]) -> image::RgbaImage {
        paint(w, h, |_, _| c)
    }

    fn land_px(m: &Mask) -> usize {
        m.land.iter().filter(|l| **l).count()
    }

    #[test]
    fn an_image_of_nothing_but_the_sea_colour_has_no_land() {
        let m = segment(&solid(8, 8, SEA), &plain());
        assert_eq!(m.land_fraction(), 0.0);
        assert_eq!(m.land.len(), 64);
    }

    #[test]
    fn an_image_with_no_sea_colour_in_it_is_all_land() {
        let m = segment(&solid(8, 8, GRASS), &plain());
        assert_eq!(m.land_fraction(), 1.0);
    }

    #[test]
    fn a_colour_near_the_sea_is_water_only_within_the_tolerance() {
        // 20 off in each channel: sqrt(3 * 400) / (sqrt(3) * 255) = 0.078 of the diagonal.
        let nearly = paint(4, 4, |_, _| [0x32, 0x4E, 0x60, 0xFF]);
        let at = |tolerance| segment(&nearly, &SeaParams { tolerance, ..plain() }).land_fraction();
        assert_eq!(at(0.16), 0.0, "well inside the default tolerance");
        assert_eq!(at(0.05), 1.0, "outside a tight one");
    }

    #[test]
    fn a_cut_out_sea_is_water_whatever_colour_is_under_it() {
        let ghost = [GRASS[0], GRASS[1], GRASS[2], 0];
        let m = segment(&paint(4, 4, |x, _| if x < 2 { ghost } else { GRASS }), &plain());
        assert!(!m.at(0, 0) && !m.at(1, 3), "transparent land colour should be sea");
        assert!(m.at(2, 0) && m.at(3, 3), "opaque land colour should be land");
    }

    #[test]
    fn the_first_row_of_the_image_is_the_north_of_the_map() {
        // y grows southward, so land in the low rows must sample as land in the low y.
        let m = segment(&paint(8, 8, |_, y| if y < 4 { GRASS } else { SEA }), &plain());
        assert!(m.sample([0.5, 0.1]), "the north half is land");
        assert!(!m.sample([0.5, 0.9]), "the south half is sea");
    }

    #[test]
    fn aspect_is_the_width_over_the_height() {
        let m = segment(&solid(40, 10, SEA), &plain());
        assert_eq!(m.aspect, 4.0);
        assert_eq!((m.w, m.h), (40, 10));
    }

    #[test]
    fn a_zero_sized_image_yields_an_empty_mask_and_a_usable_aspect() {
        let m = segment(&image::RgbaImage::new(0, 0), &SeaParams::default());
        assert!(m.land.is_empty());
        assert_eq!(m.land_fraction(), 0.0);
        assert!(m.aspect.is_finite(), "a NaN aspect would poison every distance downstream");
    }

    #[test]
    fn a_disc_of_land_in_open_sea_keeps_its_area() {
        let disc = paint(41, 41, |x, y| {
            let (dx, dy) = (x as i32 - 20, y as i32 - 20);
            if dx * dx + dy * dy <= 100 { GRASS } else { SEA }
        });
        let m = segment(&disc, &SeaParams { flood_from_edge: true, min_blob_px: 0, ..plain() });
        assert!(m.at(20, 20) && m.at(20, 11), "the disc is land");
        assert!(!m.at(0, 0) && !m.at(40, 20), "the surround is sea");
        let area = land_px(&m) as f64;
        assert!((area - std::f64::consts::PI * 100.0).abs() < 16.0, "{area} pixels is not a disc");
    }

    #[test]
    fn an_enclosed_sea_is_land_when_flooding_from_the_edge() {
        let inland_sea = |x: u32, y: u32| (9..12).contains(&x) && (9..12).contains(&y);
        let lake = paint(21, 21, |x, y| if inland_sea(x, y) { SEA } else { GRASS });

        let flooded = segment(&lake, &SeaParams { flood_from_edge: true, ..plain() });
        assert_eq!(flooded.land_fraction(), 1.0, "an inland sea is a lake, not ocean");

        let raw = segment(&lake, &plain());
        assert_eq!(land_px(&raw), 21 * 21 - 9, "without the flood the hole stays water");
        assert!(!raw.at(10, 10));
    }

    #[test]
    fn water_reaching_the_border_down_a_one_pixel_channel_is_still_sea() {
        // A bay at (5, 5) draining east to the edge along row 5.
        let bay = |mouth: u32| {
            paint(11, 11, move |x, y| if y == 5 && (5..=mouth).contains(&x) { SEA } else { GRASS })
        };
        let open = segment(&bay(10), &SeaParams { flood_from_edge: true, ..plain() });
        assert_eq!(land_px(&open), 121 - 6, "the channel keeps the bay connected to the ocean");

        let sealed = segment(&bay(9), &SeaParams { flood_from_edge: true, ..plain() });
        assert_eq!(sealed.land_fraction(), 1.0, "one pixel of land closes it into a lake");
    }

    #[test]
    fn water_touching_the_ocean_only_at_a_corner_counts_as_enclosed() {
        // Connectivity is 4-way: the diagonal is not a strait.
        let img = paint(11, 11, |x, y| match (x, y) {
            (5, 5) => SEA,
            (x, 6) if x >= 6 => SEA,
            _ => GRASS,
        });
        let m = segment(&img, &SeaParams { flood_from_edge: true, ..plain() });
        assert!(m.at(5, 5), "the diagonal pocket sank to land");
        assert!(!m.at(6, 6) && !m.at(10, 6), "the arm that reaches the border is ocean");
    }

    #[test]
    fn a_speck_of_land_is_eaten_while_an_island_of_exactly_the_threshold_is_kept() {
        let img = paint(20, 20, |x, y| match (x, y) {
            (15, 15) => GRASS,
            (x, y) if (2..7).contains(&x) && (2..7).contains(&y) => GRASS,
            _ => SEA,
        });
        let kept = segment(&img, &SeaParams { min_blob_px: 25, ..plain() });
        assert_eq!(land_px(&kept), 25, "the 5x5 island survives, the single pixel does not");
        assert!(kept.at(4, 4) && !kept.at(15, 15));

        let both = segment(&img, &plain());
        assert_eq!(land_px(&both), 26, "with no threshold the speck is an island too");
    }

    #[test]
    fn a_sea_speck_at_the_border_survives_but_one_inland_is_filled() {
        let speck = |x, y| (x, y) == (0, 0) || (x, y) == (5, 5);
        let img = paint(10, 10, |x, y| if speck(x, y) { SEA } else { GRASS });
        let m = segment(&img, &SeaParams { min_blob_px: 32, ..plain() });
        assert!(!m.at(0, 0), "open sea at the margin is not a speck, however small");
        assert!(m.at(5, 5), "a lone sea pixel inland is a compression artefact");
    }

    #[test]
    fn a_sea_pocket_is_filled_only_below_the_threshold() {
        let pocket = |side: u32| {
            paint(20, 20, move |x, y| {
                let inside = (5..5 + side).contains(&x) && (5..5 + side).contains(&y);
                if inside { SEA } else { GRASS }
            })
        };
        let small = segment(&pocket(2), &SeaParams { min_blob_px: 8, ..plain() });
        assert_eq!(small.land_fraction(), 1.0);

        let big = segment(&pocket(4), &SeaParams { min_blob_px: 8, ..plain() });
        assert_eq!(land_px(&big), 400 - 16, "a 16 pixel pocket is a lake worth keeping");
    }

    #[test]
    fn a_solid_map_the_size_of_a_real_one_floods_without_blowing_the_stack() {
        // The classic bug here is a recursive fill; 360k connected pixels would end it.
        let m = segment(&solid(600, 600, SEA), &SeaParams::default());
        assert_eq!(m.land_fraction(), 0.0);
    }

    #[test]
    fn the_same_image_segments_identically_every_time() {
        // A jittered island with scattered specks: the messy input the stage exists for.
        let mut rng = crate::rng::Rng::new(812);
        let pixels: Vec<[u8; 4]> = (0..64 * 64)
            .map(|i| {
                let (dx, dy) = (i % 64 - 32, i / 64 - 32);
                let inside = (dx * dx + dy * dy < 400) != (rng.f64() < 0.03);
                let base = if inside { GRASS } else { SEA };
                let jitter =
                    |c: u8, r: f64| (f64::from(c) + r * 24.0 - 12.0).clamp(0.0, 255.0) as u8;
                [
                    jitter(base[0], rng.f64()),
                    jitter(base[1], rng.f64()),
                    jitter(base[2], rng.f64()),
                    0xFF,
                ]
            })
            .collect();
        let img = paint(64, 64, |x, y| pixels[y as usize * 64 + x as usize]);

        let a = segment(&img, &SeaParams::default());
        let b = segment(&img, &SeaParams::default());
        assert_eq!(a.land, b.land);
        let frac = a.land_fraction();
        assert!((0.2..0.4).contains(&frac), "{frac} is not the island the fixture draws");
    }
}
