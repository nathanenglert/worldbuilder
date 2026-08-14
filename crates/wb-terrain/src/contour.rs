//! Stage 3 — contour trace, by marching squares.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::mask::Mask;

/// A closed ring of the coastline, in normalized `0..1` coordinates.
///
/// The first point is not repeated at the end; the ring is closed by definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ring {
    pub points: Vec<[f64; 2]>,
    /// True when this ring bounds water inside land — a lake shore, not a coast.
    pub is_hole: bool,
    /// Unsigned area in normalized units. Used to rank rings and to drop specks.
    pub area: f64,
}

impl Ring {
    /// Shoelace, signed. Positive means the ring winds clockwise on screen, where `y`
    /// grows downward — which is how an *outer* boundary comes out of the tracer below.
    pub fn signed_area(points: &[[f64; 2]]) -> f64 {
        let n = points.len();
        if n < 3 {
            return 0.0;
        }
        let mut sum = 0.0;
        for i in 0..n {
            let a = points[i];
            let b = points[(i + 1) % n];
            sum += a[0] * b[1] - b[0] * a[1];
        }
        sum / 2.0
    }
}

/// Rings under this many normalized square units are dropped. A lone pixel traces about
/// `0.5 / (w * h)`, so on any raster worth calling a map this keeps specks out without
/// touching an island anyone would name.
const MIN_AREA: f64 = 1e-7;

/// A crossing, in half-pixel integer units: `(2 * px, 2 * py)`. Segments are linked by
/// exact integer equality on these, never by comparing the floats they become.
type Cross = (i64, i64);

/// Trace every boundary between land and sea.
///
/// Marching squares over the 2×2 windows of `mask`, walking each contour so that **land
/// is on the left**. Under that rule the winding tells outer rings from holes without any
/// point-in-polygon nesting test: outer rings come out one way, holes the other.
///
/// Requirements:
///
/// - Output coordinates are normalized: pixel `(x, y)` maps to `(x / w, y / h)`, so a
///   contour on the `x = 0` boundary lands at `0.0` and one at `x = w` lands at `1.0`.
/// - Every ring is closed and has at least 4 points; degenerate ones are dropped.
/// - `area` is `signed_area(...).abs()`, and `is_hole` follows the sign.
/// - Rings come back sorted by `area`, largest first. The mainland should be `[0]`.
/// - The saddle cases (mask patterns 5 and 10) must be resolved consistently, or a
///   diagonal pixel bridge will produce a ring that crosses itself.
///
/// Two consequences of the above, spelled out because callers depend on them:
///
/// - Left is meant in the plane's own orientation, so an outer ring comes back with
///   positive [`Ring::signed_area`] — clockwise on screen — and a hole with negative.
///   `is_hole` is exactly that sign.
/// - Vertices sit on the midpoints of pixel *edges*, not on pixel corners: the crossing
///   between pixels `(x, y)` and `(x + 1, y)` is at `((x + 1) / w, (y + 0.5) / h)`. Half a
///   pixel off and a one-pixel lake stops closing.
pub fn trace(mask: &Mask) -> Vec<Ring> {
    if mask.w == 0 || mask.h == 0 {
        return Vec::new();
    }
    let (w, h) = (i64::from(mask.w), i64::from(mask.h));
    let segments = march(mask, w, h);

    let mut leaving: BTreeMap<Cross, usize> = BTreeMap::new();
    for (i, (from, _)) in segments.iter().enumerate() {
        leaving.entry(*from).or_insert(i);
    }

    let (sx, sy) = (1.0 / (2.0 * w as f64), 1.0 / (2.0 * h as f64));
    let mut used = vec![false; segments.len()];
    let mut rings: Vec<Ring> = Vec::new();

    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        // Iterative by construction: a coastline is millions of segments long and a
        // recursive walk would take the stack with it.
        let mut points: Vec<[f64; 2]> = Vec::new();
        let mut at = start;
        loop {
            used[at] = true;
            let (from, to) = segments[at];
            points.push([from.0 as f64 * sx, from.1 as f64 * sy]);
            match leaving.get(&to) {
                // Exactly one segment leaves every crossing, so the only way to stop is
                // to arrive back at `start`.
                Some(&next) if !used[next] => at = next,
                _ => break,
            }
        }

        if points.len() < 4 {
            continue;
        }
        let signed = Ring::signed_area(&points);
        if signed.abs() < MIN_AREA {
            continue;
        }
        rings.push(Ring { points, is_hole: signed < 0.0, area: signed.abs() });
    }

    // Stable, so rings of equal area keep the sweep order they were found in.
    rings.sort_by(|a, b| b.area.total_cmp(&a.area));
    rings
}

/// Every directed boundary segment, in sweep order.
///
/// The sweep runs one window past each edge of the raster. [`Mask::at`] reports sea off
/// the image, so a continent running off the map is closed along the border instead of
/// leaking an open contour.
fn march(mask: &Mask, w: i64, h: i64) -> Vec<(Cross, Cross)> {
    let mut segments = Vec::new();

    for y in -1..h {
        // Carry the previous window's right column, so each pixel is read twice over the
        // whole sweep rather than four times.
        let mut tl = mask.at(-1, y);
        let mut bl = mask.at(-1, y + 1);

        for x in -1..w {
            let tr = mask.at(x + 1, y);
            let br = mask.at(x + 1, y + 1);
            let case =
                (u8::from(tl) << 3) | (u8::from(tr) << 2) | (u8::from(br) << 1) | u8::from(bl);
            tl = tr;
            bl = br;

            if case == 0 || case == 15 {
                continue;
            }

            // The four crossings around this window, as midpoints of the pixel edges they
            // cut. They form a diamond around the corner the four pixels share.
            let n = (2 * x + 2, 2 * y + 1);
            let e = (2 * x + 3, 2 * y + 2);
            let s = (2 * x + 2, 2 * y + 3);
            let west = (2 * x + 1, 2 * y + 2);

            // Directions put land on the left, which reads clockwise on screen around an
            // island. Cases 5 and 10 are the saddles: both bridge the land and cut the sea
            // apart — the same choice twice, or a diagonal chain of pixels would hand back
            // a ring that crosses itself.
            match case {
                1 => segments.push((west, s)),
                2 => segments.push((s, e)),
                3 => segments.push((west, e)),
                4 => segments.push((e, n)),
                5 => segments.extend([(west, n), (e, s)]),
                6 => segments.push((s, n)),
                7 => segments.push((west, n)),
                8 => segments.push((n, west)),
                9 => segments.push((n, s)),
                10 => segments.extend([(n, e), (s, west)]),
                11 => segments.push((n, e)),
                12 => segments.push((e, west)),
                13 => segments.push((e, s)),
                _ => segments.push((s, west)),
            }
        }
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `#` is land, `.` is sea. One row per raster row, so the literal reads like the map.
    fn mask(rows: &[&str]) -> Mask {
        let h = rows.len() as u32;
        let w = rows[0].len() as u32;
        let mut land = Vec::with_capacity(rows.len() * w as usize);
        for r in rows {
            assert_eq!(r.len() as u32, w, "ragged mask literal");
            land.extend(r.chars().map(|c| c == '#'));
        }
        Mask { w, h, aspect: f64::from(w) / f64::from(h), land }
    }

    /// Areas are far easier to check in whole pixels than in normalized units.
    fn px(area: f64, m: &Mask) -> f64 {
        area * f64::from(m.w) * f64::from(m.h)
    }

    fn annulus() -> Mask {
        mask(&[".......", ".#####.", ".#####.", ".##.##.", ".#####.", ".#####.", "......."])
    }

    #[test]
    fn a_mask_with_no_land_has_no_rings() {
        assert!(trace(&mask(&["....", "....", "...."])).is_empty());
    }

    #[test]
    fn a_filled_rectangle_is_one_ring_that_is_not_a_hole() {
        let m = mask(&["......", ".####.", ".####.", ".####.", ".####.", "......"]);
        let rings = trace(&m);
        assert_eq!(rings.len(), 1);

        let r = &rings[0];
        assert!(!r.is_hole);
        // 16 land pixels, less the half-pixel triangle marching squares cuts off each
        // corner: 16 - 4 * 0.125.
        assert!((px(r.area, &m) - 15.5).abs() < 1e-9, "{} px²", px(r.area, &m));
        // One crossing per land pixel on each of the four sides.
        assert_eq!(r.points.len(), 16);
        assert_ne!(r.points.first(), r.points.last(), "the ring must not repeat its start");
    }

    #[test]
    fn an_outer_ring_winds_positive_and_a_hole_winds_negative() {
        // The sign is the whole of `is_hole`, so it is pinned here rather than assumed.
        let rings = trace(&annulus());
        assert!(Ring::signed_area(&rings[0].points) > 0.0, "an island winds clockwise on screen");
        assert!(Ring::signed_area(&rings[1].points) < 0.0, "a lake shore winds the other way");
    }

    #[test]
    fn a_ring_of_water_inside_land_is_a_hole() {
        let m = annulus();
        let rings = trace(&m);
        assert_eq!(rings.len(), 2);
        assert!(!rings[0].is_hole, "the coast bounds land");
        assert!(rings[1].is_hole, "the lake shore bounds water");
        assert!(rings[0].area > rings[1].area, "the coast is the larger of the two");
        assert!((px(rings[0].area, &m) - 24.5).abs() < 1e-9, "{} px²", px(rings[0].area, &m));
        assert!((px(rings[1].area, &m) - 0.5).abs() < 1e-9, "{} px²", px(rings[1].area, &m));
    }

    #[test]
    fn two_separate_islands_come_back_largest_first() {
        let m = mask(&["........", ".##.....", ".##.....", "........", "......#.", "........"]);
        let rings = trace(&m);
        assert_eq!(rings.len(), 2);
        assert!(rings.iter().all(|r| !r.is_hole), "neither island bounds water");
        assert!((px(rings[0].area, &m) - 3.5).abs() < 1e-9, "{} px²", px(rings[0].area, &m));
        assert!((px(rings[1].area, &m) - 0.5).abs() < 1e-9, "{} px²", px(rings[1].area, &m));
    }

    #[test]
    fn land_running_off_the_image_edge_still_closes() {
        let m = mask(&["##..", "##..", "...."]);
        let rings = trace(&m);
        assert_eq!(rings.len(), 1);

        let r = &rings[0];
        assert!(!r.is_hole);
        assert!((px(r.area, &m) - 3.5).abs() < 1e-9, "{} px²", px(r.area, &m));
        assert!(r.points.iter().any(|p| p[0] == 0.0), "the west coast sits on the image edge");
        assert!(r.points.iter().any(|p| p[1] == 0.0), "and so does the north coast");
        assert!(
            r.points.iter().all(|p| (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1])),
            "no point escaped the unit square"
        );

        // Closure, including across the seam: every step crosses one window, which is
        // exactly one pixel of Manhattan travel. A ring that failed to close would show a
        // long jump somewhere in the cycle.
        for i in 0..r.points.len() {
            let a = r.points[i];
            let b = r.points[(i + 1) % r.points.len()];
            let dx = (a[0] - b[0]).abs() * f64::from(m.w);
            let dy = (a[1] - b[1]).abs() * f64::from(m.h);
            assert!((dx + dy - 1.0).abs() < 1e-9, "gap of {dx}, {dy} px between {a:?} and {b:?}");
        }
    }

    #[test]
    fn land_touching_at_a_corner_traces_as_one_ring() {
        // Case 10: the saddle bridges the land, so the two pixels share a coastline.
        let m = mask(&[".....", ".#...", "..#..", ".....", "....."]);
        let rings = trace(&m);
        assert_eq!(rings.len(), 1, "a diagonal touch must not split into two islands");
        assert!(!rings[0].is_hole);
        assert_eq!(rings[0].points.len(), 8);
        // Two pixels of land plus the bridge across the saddle, less the chamfers.
        assert!((px(rings[0].area, &m) - 1.5).abs() < 1e-9, "{} px²", px(rings[0].area, &m));
    }

    #[test]
    fn water_touching_at_a_corner_traces_as_two_holes() {
        // Case 5, the other saddle, resolved the same way: land stays connected across the
        // diagonal, which is what keeps the two lakes apart.
        let m = mask(&["#####", "#.###", "##.##", "#####", "#####"]);
        let rings = trace(&m);
        assert_eq!(rings.len(), 3, "the border ring and one ring per lake");
        assert!(!rings[0].is_hole, "the coastline follows the image border");
        assert!(rings[1].is_hole && rings[2].is_hole);
        assert!((px(rings[1].area, &m) - 0.5).abs() < 1e-9);
        assert!((px(rings[2].area, &m) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn every_vertex_sits_on_the_midpoint_of_a_pixel_edge() {
        let m = annulus();
        let whole = |v: f64| (v - v.round()).abs() < 1e-9;
        let halfway = |v: f64| ((v - 0.5) - (v - 0.5).round()).abs() < 1e-9;

        for r in trace(&m) {
            for p in &r.points {
                let (x, y) = (p[0] * f64::from(m.w), p[1] * f64::from(m.h));
                assert!(
                    (whole(x) && halfway(y)) || (halfway(x) && whole(y)),
                    "{p:?} is at ({x}, {y}) px, which is not a pixel-edge midpoint"
                );
            }
        }
    }

    #[test]
    fn a_speck_is_dropped_but_a_real_island_on_the_same_raster_is_not() {
        // A lone pixel over 2600×2000 covers 9.6e-8 — under the threshold, and exactly the
        // compression artefact that would otherwise be reported as an island.
        let (w, h) = (2600u32, 2000u32);
        let mut land = vec![false; (w * h) as usize];
        land[900 * w as usize + 400] = true;
        for y in 1000..1004 {
            for x in 1000..1004 {
                land[y * w as usize + x] = true;
            }
        }
        let m = Mask { w, h, aspect: f64::from(w) / f64::from(h), land };

        let rings = trace(&m);
        assert_eq!(rings.len(), 1, "only the 4×4 island survives");
        assert!((px(rings[0].area, &m) - 15.5).abs() < 1e-6, "{} px²", px(rings[0].area, &m));
    }

    #[test]
    fn the_same_mask_traces_identically_every_time() {
        let m = annulus();
        let (a, b) = (trace(&m), trace(&m));
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.points, y.points);
            assert_eq!(x.is_hole, y.is_hole);
            assert_eq!(x.area, y.area);
        }
    }
}
