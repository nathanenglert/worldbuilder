//! Stage 6 — elevation.
//!
//! Coastal falloff plus hand-placed ranges plus noise. Deliberately not a plate-tectonic
//! simulation: the coastline is already given by the writer's map, and a simulation that
//! disagreed with it would have to be overruled everywhere anyway.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::cells::Substrate;
use crate::mask::Mask;
use crate::params::HeightParams;
use crate::rng::{Digest, Rng};

/// Octaves of value noise. Four spans continent-scale swells down to about one cell at
/// the default density; a fifth is invisible once step 5 has relaxed the field.
const OCTAVES: u32 = 4;
/// Lattice cells across the map's height at the coarsest octave.
const BASE_LATTICE: usize = 4;
const RELAX_PASSES: usize = 2;
const RELAX_MIX: f64 = 0.25;
/// The shortest distance over which noise may fade in from the coast, in normalized
/// units — a cell or two at the usual densities. Without a floor, a cliff coast (a tiny
/// `shelf`) would get full-amplitude noise right at the shoreline.
const COAST_TAPER_MIN: f64 = 0.03;
/// How far apart step 6 holds the two sides of the shore.
const MARGIN: f32 = 1e-4;

/// Height per cell, on a `0.0..=1.0` scale where `p.sea_level` is the shore.
///
/// Requirements, in order:
///
/// 1. **Distance to the coast**, per cell, by breadth-first search over `neighbors`
///    starting from every cell whose neighbourhood contains both land and sea. Accumulate
///    [`Substrate::distance`], not hop count — cell spacing is even but not uniform.
/// 2. **Falloff.** Land rises as `sea_level + (1 - sea_level) * (1 - exp(-d / shelf))`,
///    sea falls as `sea_level * exp(-d / shelf)`. Coast is exactly `sea_level` on both
///    sides, so the shore never disagrees with the traced coastline.
/// 3. **Ranges.** For each [`crate::params::Range`], add uplift falling off with distance
///    from the segment `from..to`, reaching `peak` on the ridge line and zero at `width`.
///    Use a smooth falloff, not linear, or the range gets a visible faceted edge. Ranges
///    add to the falloff rather than replacing it, and only over land.
/// 4. **Noise.** Value noise from [`crate::rng::Rng`] seeded with `p`'s owner seed,
///    summed over a few octaves, scaled by `roughness`, and tapered to zero at the coast
///    so it cannot push a shoreline cell to the wrong side of sea level.
/// 5. **Relax.** One or two passes averaging each land cell a little toward its
///    neighbours, to kill single-cell spikes that would each become a river source.
/// 6. Clamp to `0.0..=1.0`, and — this is the invariant everything downstream leans on —
///    guarantee `is_land[i] == (height[i] > sea_level)` for every cell.
///
/// `seed` is passed separately because [`HeightParams`] is the writer's half of the
/// input and the seed belongs to the world.
///
/// The result has one entry per cell, every one of them finite and inside `0.0..=1.0`.
/// A `sea_level` outside `0.0..=1.0` is pulled just inside it, since step 6 needs room on
/// both sides of the shore and the `0..=1` scale is what downstream stages assume.
pub fn heights(sub: &Substrate, mask: &Mask, p: &HeightParams, seed: u64) -> Vec<f32> {
    let n = sub.len();
    if n == 0 {
        return Vec::new();
    }

    // Normalized coordinates squash a non-square map and every distance here has to undo
    // that. `Substrate::distance` does it for graph edges; ranges and the noise lattice
    // take the raster's own aspect. A degenerate mask must not poison the whole field.
    let aspect = if mask.aspect.is_finite() && mask.aspect > 0.0 { mask.aspect } else { 1.0 };
    // Step 6 needs room on both sides of the shore, and `clamp` would hand a NaN straight
    // through into every cell.
    let shore = if p.sea_level.is_finite() {
        p.sea_level.clamp(MARGIN, 1.0 - MARGIN)
    } else {
        HeightParams::default().sea_level
    };
    let sea_level = f64::from(shore);
    let shelf = p.shelf.max(1e-9); // `d / shelf` must never be 0/0 at the coast

    let dist = coast_distance(sub);

    // Step 2. Both branches meet at exactly `sea_level` where `d` is zero, so the shore
    // agrees with the traced coastline whatever the shelf is.
    let mut h: Vec<f64> = (0..n)
        .map(|i| {
            let falloff = (-dist[i] / shelf).exp();
            if sub.is_land[i] {
                sea_level + (1.0 - sea_level) * (1.0 - falloff)
            } else {
                sea_level * falloff
            }
        })
        .collect();

    // Step 3.
    for r in &p.ranges {
        if !r.width.is_finite() || r.width <= 0.0 {
            continue;
        }
        for (i, hi) in h.iter_mut().enumerate() {
            if !sub.is_land[i] {
                continue;
            }
            let d = segment_distance(sub.sites[i], r.from, r.to, aspect);
            let lift = f64::from(r.peak) * smoothstep(1.0 - d / r.width);
            *hi = (*hi + lift).clamp(0.0, 1.0);
        }
    }

    // Step 4.
    if p.roughness != 0.0 && p.roughness.is_finite() {
        let field = Fractal::new(seed, aspect);
        let taper = shelf.max(COAST_TAPER_MIN);
        for (i, hi) in h.iter_mut().enumerate() {
            let signed = field.at(sub.sites[i]) - 0.5;
            *hi = (*hi + p.roughness * signed * smoothstep(dist[i] / taper)).clamp(0.0, 1.0);
        }
    }

    // Step 5. Jacobi rather than in-place, so the sweep order cannot reach the output.
    // Sea neighbours are left out: they sit at the shore by construction and averaging
    // against them would plane every island back down to it.
    for _ in 0..RELAX_PASSES {
        let prev = h.clone();
        for (i, hi) in h.iter_mut().enumerate() {
            if !sub.is_land[i] {
                continue;
            }
            let (mut sum, mut count) = (0.0, 0u32);
            for &nb in &sub.neighbors[i] {
                if sub.is_land[nb as usize] {
                    sum += prev[nb as usize];
                    count += 1;
                }
            }
            if count > 0 {
                *hi = (*hi + RELAX_MIX * (sum / f64::from(count) - *hi)).clamp(0.0, 1.0);
            }
        }
    }

    // Step 6. `f32::max` and `f32::min` return the other operand when one side is NaN, so
    // this is also the last gate a NaN could have escaped through.
    h.iter()
        .zip(&sub.is_land)
        .map(|(v, land)| {
            let v = *v as f32;
            if *land { v.max(shore + MARGIN).min(1.0) } else { v.min(shore - MARGIN).max(0.0) }
        })
        .collect()
}

/// Step 1: metric distance from every cell to the nearest coastal one.
///
/// Dijkstra rather than a plain queue, because the edge weights are the varying distances
/// between sites; an explicit heap rather than recursion, because a real map is 3,000
/// cells deep in places and the stack is not.
fn coast_distance(sub: &Substrate) -> Vec<f64> {
    let n = sub.len();
    let mut dist = vec![f64::INFINITY; n];
    let mut queue: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::new();

    for (i, d) in dist.iter_mut().enumerate() {
        let shore = sub.neighbors[i].iter().any(|&j| sub.is_land[j as usize] != sub.is_land[i]);
        if shore {
            *d = 0.0;
            queue.push(Reverse((0, i as u32)));
        }
    }
    if queue.is_empty() {
        // All land or all sea: there is no shore to measure from, and every cell is
        // equally far from it. Anything else here is an infinite loop or a NaN.
        return vec![0.0; n];
    }

    // Distances are non-negative and finite, and `to_bits` is monotonic over those, which
    // buys a total order on the heap without wrapping f64 in an Ord newtype.
    while let Some(Reverse((bits, i))) = queue.pop() {
        let (d, i) = (f64::from_bits(bits), i as usize);
        if d > dist[i] {
            continue;
        }
        for &nb in &sub.neighbors[i] {
            let nb = nb as usize;
            let cand = d + sub.distance(i, nb);
            if cand < dist[nb] {
                dist[nb] = cand;
                queue.push(Reverse((cand.to_bits(), nb as u32)));
            }
        }
    }
    dist
}

/// Distance from a point to the segment `a..b`, with the horizontal squash undone.
fn segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2], aspect: f64) -> f64 {
    let (px, py) = (p[0] * aspect, p[1]);
    let (ax, ay) = (a[0] * aspect, a[1]);
    let (dx, dy) = ((b[0] - a[0]) * aspect, b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t =
        if len2 > 0.0 { (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    f64::hypot(px - (ax + t * dx), py - (ay + t * dy))
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Value noise: one lattice of random values per octave, smoothstep-interpolated. Value
/// rather than gradient noise because the taper and the relaxation pass flatten the
/// differences anyway, and this is eight lines that can never drift between releases.
struct Lattice {
    nx: usize,
    ny: usize,
    amp: f64,
    v: Vec<f64>,
}

impl Lattice {
    fn new(nx: usize, ny: usize, amp: f64, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let v = (0..(nx + 1) * (ny + 1)).map(|_| rng.f64()).collect();
        Self { nx, ny, amp, v }
    }

    /// Interpolated value at a normalized point, in `[0, 1)`.
    fn at(&self, p: [f64; 2]) -> f64 {
        let gx = p[0].clamp(0.0, 1.0) * self.nx as f64;
        let gy = p[1].clamp(0.0, 1.0) * self.ny as f64;
        // Saturating float-to-int casts make this safe for a NaN site as well as a corner.
        let i = (gx as usize).min(self.nx - 1);
        let j = (gy as usize).min(self.ny - 1);
        let row = self.nx + 1;
        // Smoothstep, not linear: linear interpolation creases visibly at every lattice
        // line, and the creases survive into the rivers as a grid of parallel valleys.
        let (tx, ty) = (smoothstep(gx - i as f64), smoothstep(gy - j as f64));
        let lo = lerp(self.v[j * row + i], self.v[j * row + i + 1], tx);
        let hi = lerp(self.v[(j + 1) * row + i], self.v[(j + 1) * row + i + 1], tx);
        lerp(lo, hi, ty)
    }
}

struct Fractal {
    octaves: Vec<Lattice>,
    norm: f64,
}

impl Fractal {
    fn new(seed: u64, aspect: f64) -> Self {
        let mut octaves = Vec::with_capacity(OCTAVES as usize);
        let (mut amp, mut norm) = (1.0, 0.0);
        for k in 0..OCTAVES {
            let ny = BASE_LATTICE << k;
            // Square lattice cells in aspect-corrected space, so the noise does not come
            // out visibly stretched on a wide map. Bounded, since aspect is a parameter.
            let nx = ((ny as f64 * aspect).round() as usize).clamp(1, 1024);
            // Every octave gets its own stream, so adding one never reshuffles the others.
            octaves.push(Lattice::new(nx, ny, amp, Digest::new().u64(seed).u64(k.into()).finish()));
            norm += amp;
            amp *= 0.5;
        }
        Self { octaves, norm }
    }

    /// Summed octaves, renormalized back into `[0, 1)`.
    fn at(&self, p: [f64; 2]) -> f64 {
        self.octaves.iter().map(|o| o.amp * o.at(p)).sum::<f64>() / self.norm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::Range;

    /// A `cols × rows` lattice of square cells with four-way adjacency — the substrate a
    /// Poisson-disc mesh approximates, with every distance known by hand.
    fn grid(cols: usize, rows: usize, aspect: f64, land: impl Fn(usize, usize) -> bool) -> Parts {
        let (mut sites, mut polygons) = (Vec::new(), Vec::new());
        let (mut neighbors, mut is_land) = (Vec::new(), Vec::new());
        for r in 0..rows {
            for c in 0..cols {
                let (x0, y0) = (c as f64 / cols as f64, r as f64 / rows as f64);
                let (x1, y1) = ((c + 1) as f64 / cols as f64, (r + 1) as f64 / rows as f64);
                sites.push([(x0 + x1) / 2.0, (y0 + y1) / 2.0]);
                polygons.push(vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]);

                let mut nb = Vec::new();
                let mut push = |c: usize, r: usize| nb.push((r * cols + c) as u32);
                if c > 0 {
                    push(c - 1, r);
                }
                if c + 1 < cols {
                    push(c + 1, r);
                }
                if r > 0 {
                    push(c, r - 1);
                }
                if r + 1 < rows {
                    push(c, r + 1);
                }
                neighbors.push(nb);
                is_land.push(land(c, r));
            }
        }
        let mask = Mask { w: cols as u32, h: rows as u32, aspect, land: is_land.clone() };
        Parts { sub: Substrate { aspect, sites, polygons, neighbors, is_land }, mask, cols }
    }

    struct Parts {
        sub: Substrate,
        mask: Mask,
        cols: usize,
    }

    impl Parts {
        fn at(&self, c: usize, r: usize) -> usize {
            r * self.cols + c
        }
    }

    /// A west-facing shore: the first four columns are sea, the rest is land.
    fn coastline(cols: usize, rows: usize) -> Parts {
        grid(cols, rows, 1.4, |c, _| c >= 4)
    }

    fn params() -> HeightParams {
        HeightParams { roughness: 0.0, ..HeightParams::default() }
    }

    #[test]
    fn height_rises_monotonically_inland_along_a_transect() {
        let g = coastline(20, 6);
        let p = params();
        let h = heights(&g.sub, &g.mask, &p, 1);

        for c in 4..19 {
            let (here, next) = (h[g.at(c, 2)], h[g.at(c + 1, 2)]);
            assert!(next > here, "column {c} at {here} is not below column {} at {next}", c + 1);
        }
        assert!(h[g.at(4, 2)] > p.sea_level, "the first land column is above the shore");
    }

    #[test]
    fn the_middle_of_an_island_is_its_highest_ground() {
        // Radial, not a transect: the coast distance has to come out of the graph search
        // rather than out of one axis.
        let g = grid(21, 21, 1.0, |c, r| {
            let (dc, dr) = (c as f64 - 10.0, r as f64 - 10.0);
            dc * dc + dr * dr <= 64.0
        });
        let h = heights(&g.sub, &g.mask, &params(), 1);

        let top = h.iter().copied().fold(f32::MIN, f32::max);
        assert_eq!(h[g.at(10, 10)], top, "the summit should be the middle of the island");
        for r in [10, 13, 16] {
            let (here, out) = (h[g.at(10, r)], h[g.at(10, r + 2)]);
            assert!(here > out, "row {r} at {here} should stand above row {} at {out}", r + 2);
        }
    }

    #[test]
    fn the_sea_floor_falls_away_from_the_shore() {
        let g = coastline(20, 6);
        let h = heights(&g.sub, &g.mask, &params(), 1);

        for c in 0..3 {
            assert!(h[g.at(c, 2)] < h[g.at(c + 1, 2)], "the sea should deepen westward");
        }
        assert!(h[g.at(0, 2)] < h[g.at(3, 2)] * 0.9, "a wide shelf, not a wall");
    }

    #[test]
    fn a_narrow_shelf_makes_the_land_rise_faster_than_a_wide_one() {
        let g = coastline(20, 6);
        let cliff = HeightParams { shelf: 0.02, ..params() };
        let ramp = HeightParams { shelf: 0.4, ..params() };

        let (a, b) = (heights(&g.sub, &g.mask, &cliff, 1), heights(&g.sub, &g.mask, &ramp, 1));
        assert!(a[g.at(6, 2)] > b[g.at(6, 2)], "the cliff should already be high two cells in");
        assert!(a[g.at(19, 2)] > 0.99, "and saturated at the far side");
    }

    #[test]
    fn a_range_lifts_the_ground_under_its_ridge_above_the_ground_beside_it() {
        // The shore runs north-south, so coastal distance depends only on the column and
        // any difference within a column is the range's doing.
        let g = coastline(20, 9);
        let ridge = Range {
            name: "Spine".into(),
            from: [0.3, 0.5],
            to: [0.9, 0.5],
            peak: 0.4,
            width: 0.15,
        };
        let p = HeightParams { ranges: vec![ridge], ..params() };
        let h = heights(&g.sub, &g.mask, &p, 1);
        let flat = heights(&g.sub, &g.mask, &params(), 1);

        let (under, beside) = (h[g.at(12, 4)], h[g.at(12, 0)]);
        assert!(under > beside, "the ridge at {under} is not above the flank at {beside}");
        assert!(
            under > flat[g.at(12, 4)],
            "the range adds to the falloff rather than replacing it"
        );
        assert!(
            (beside - flat[g.at(12, 0)]).abs() < 1e-6,
            "the far flank is outside the range's width and should not move"
        );
    }

    #[test]
    fn a_range_never_lifts_the_sea() {
        let g = coastline(20, 6);
        let over_water = Range {
            name: "Drowned".into(),
            from: [0.0, 0.5],
            to: [1.0, 0.5],
            peak: 0.9,
            width: 0.9,
        };
        let p = HeightParams { ranges: vec![over_water], ..params() };
        let h = heights(&g.sub, &g.mask, &p, 1);
        let flat = heights(&g.sub, &g.mask, &params(), 1);

        for (i, v) in h.iter().enumerate() {
            if !g.sub.is_land[i] {
                assert_eq!(*v, flat[i], "cell {i} is sea and the range should not reach it");
            }
        }
    }

    #[test]
    fn every_cell_lands_on_the_side_of_sea_level_its_mask_says() {
        // Two islands, a strait, and a lagoon inside the larger one — every kind of
        // neighbourhood the invariant has to survive.
        let g = grid(24, 16, 1.5, |c, r| {
            let big = (4..14).contains(&c) && (3..13).contains(&r);
            let lagoon = (7..10).contains(&c) && (6..9).contains(&r);
            let small = (18..22).contains(&c) && (5..9).contains(&r);
            (big && !lagoon) || small
        });
        let p = HeightParams {
            roughness: 1.0,
            ranges: vec![Range {
                name: "Everywhere".into(),
                from: [0.0, 0.0],
                to: [1.0, 1.0],
                peak: 0.8,
                width: 0.9,
            }],
            ..HeightParams::default()
        };
        let h = heights(&g.sub, &g.mask, &p, 812);

        for (i, v) in h.iter().enumerate() {
            let land = g.sub.is_land[i];
            assert_eq!(land, *v > p.sea_level, "cell {i} is_land={land} but sits at {v}");
        }
    }

    #[test]
    fn every_height_is_finite_and_inside_the_unit_interval() {
        let g = grid(18, 12, 2.0, |c, r| c * r % 5 != 0);
        let p = HeightParams {
            roughness: 4.0,
            shelf: 0.001,
            ranges: vec![Range {
                name: "Overshoot".into(),
                from: [0.2, 0.2],
                to: [0.8, 0.8],
                peak: 5.0,
                width: 2.0,
            }],
            ..HeightParams::default()
        };
        let h = heights(&g.sub, &g.mask, &p, 3);

        assert_eq!(h.len(), g.sub.len());
        for (i, v) in h.iter().enumerate() {
            assert!(v.is_finite() && (0.0..=1.0).contains(v), "cell {i} escaped the scale at {v}");
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_field_and_a_different_seed_does_not() {
        let g = coastline(20, 12);
        let p = HeightParams { roughness: 0.3, ..HeightParams::default() };

        assert_eq!(heights(&g.sub, &g.mask, &p, 7), heights(&g.sub, &g.mask, &p, 7));
        assert_ne!(heights(&g.sub, &g.mask, &p, 7), heights(&g.sub, &g.mask, &p, 8));
    }

    #[test]
    fn smooth_terrain_ignores_the_seed_entirely() {
        let g = coastline(20, 12);
        let p = params();
        assert_eq!(heights(&g.sub, &g.mask, &p, 1), heights(&g.sub, &g.mask, &p, 99));

        // And a smooth field really is smooth: with the shore running north-south, one
        // column is one height. Not to the bit — the relaxation pass has one neighbour
        // fewer to average against at the top and bottom of the mesh — but to a thousandth.
        let h = heights(&g.sub, &g.mask, &p, 1);
        for r in 1..12 {
            let d = (h[g.at(9, r)] - h[g.at(9, 0)]).abs();
            assert!(d < 1e-3, "row {r} of column 9 is {d} away from row 0");
        }
    }

    #[test]
    fn noise_fades_out_at_the_coast() {
        let g = coastline(20, 12);
        let rough = HeightParams { roughness: 0.8, ..HeightParams::default() };
        let (h, flat) =
            (heights(&g.sub, &g.mask, &rough, 5), heights(&g.sub, &g.mask, &params(), 5));

        let moved = |c: usize| {
            (0..12).map(|r| (h[g.at(c, r)] - flat[g.at(c, r)]).abs()).fold(0.0, f32::max)
        };
        let (shore, inland) = (moved(4), moved(15));
        assert!(inland > 0.05, "the noise should be doing something inland: {inland}");
        assert!(shore < inland * 0.25, "the shore moved {shore} against {inland} inland");
    }

    #[test]
    fn a_map_with_no_shore_at_all_comes_back_flat() {
        let all_land = grid(8, 8, 1.0, |_, _| true);
        let all_sea = grid(8, 8, 1.0, |_, _| false);
        let p = HeightParams { roughness: 0.5, ..HeightParams::default() };

        let dry = heights(&all_land.sub, &all_land.mask, &p, 1);
        let wet = heights(&all_sea.sub, &all_sea.mask, &p, 1);
        assert!(dry.iter().all(|v| *v == dry[0] && *v > p.sea_level), "{dry:?}");
        assert!(wet.iter().all(|v| *v == wet[0] && *v < p.sea_level), "{wet:?}");
    }

    #[test]
    fn a_sea_level_at_the_edge_of_the_scale_still_leaves_room_for_the_shore() {
        let g = coastline(12, 6);
        for level in [0.0, 1.0e-9, 0.999_999] {
            let p = HeightParams { sea_level: level, ..HeightParams::default() };
            let h = heights(&g.sub, &g.mask, &p, 2);
            for (i, v) in h.iter().enumerate() {
                assert!((0.0..=1.0).contains(v), "cell {i} at {v} with sea_level {level}");
                assert_eq!(g.sub.is_land[i], *v > p.sea_level, "cell {i} at sea_level {level}");
            }
        }
    }

    #[test]
    fn relaxation_pulls_a_lone_spike_back_toward_its_neighbours() {
        // A one-cell-wide range is exactly the spike step 5 exists to blunt: without the
        // relaxation pass the peak would stand at its full uplift.
        let g = coastline(20, 9);
        let spike = Range {
            name: "Needle".into(),
            from: [0.6, 0.5],
            to: [0.6, 0.5],
            peak: 0.5,
            width: 0.04,
        };
        let p = HeightParams { ranges: vec![spike], ..params() };
        let h = heights(&g.sub, &g.mask, &p, 1);
        let flat = heights(&g.sub, &g.mask, &params(), 1);

        let peak = g.at(12, 4);
        let lift = h[peak] - flat[peak];
        assert!(lift > 0.0, "the spike should still be a hill");
        assert!(lift < 0.5, "but blunted from its {} of uplift, not left at it", 0.5);
        assert!(h[g.at(12, 3)] > flat[g.at(12, 3)], "and it should have spread to a neighbour");
    }
}
