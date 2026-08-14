//! Stage 4 — simplification, by Visvalingam–Whyatt.
//!
//! Visvalingam rather than Douglas–Peucker because it degrades better: dropping the
//! least-significant *area* keeps a coastline looking like a coastline at low detail,
//! where dropping by perpendicular distance turns bays into chevrons.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

use crate::contour::Ring;

/// A ring smaller than this is a speck: a few pixels across on any raster a writer would
/// hand us, and an island nobody drew.
const MIN_RING_AREA: f64 = 1e-7;

/// Simplify one closed ring. `epsilon` is an effective-area threshold in normalized
/// square units; points whose triangle is smaller than it are removed, cheapest first.
///
/// Requirements:
///
/// - Treat the ring as closed: the first and last points have neighbours too, so a
///   feature straddling the seam is not preserved by accident.
/// - Recompute the neighbours' effective areas after each removal, and never let a
///   recomputed area fall below the one just removed — otherwise simplification is
///   order-dependent and the "detail" slider stops being monotonic.
/// - Never return fewer than 4 points.
/// - `aspect` corrects the horizontal squash of normalized coordinates before areas are
///   measured, so a wide map does not simplify differently along `x` than along `y`.
///
/// The ring is held as a doubly linked list so a removal is O(1), and the candidates in
/// a binary heap. Superseded heap entries are left where they are and skipped when they
/// surface, which is cheaper than hunting them down.
pub fn simplify_ring(points: &[[f64; 2]], epsilon: f64, aspect: f64) -> Vec<[f64; 2]> {
    let n = points.len();
    if n <= 4 {
        return points.to_vec();
    }
    // A NaN threshold compares false against every area, which would silently shave every
    // ring down to its four-point floor. Keeping everything is the safer reading.
    let epsilon = if epsilon.is_nan() { 0.0 } else { epsilon };

    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut alive = vec![true; n];
    // The generation of the one heap entry per point that still counts.
    let mut generation = vec![0u32; n];

    let mut heap: BinaryHeap<Reverse<Candidate>> = (0..n)
        .map(|i| {
            let area = triangle_area(points[prev[i]], points[i], points[next[i]], aspect);
            Reverse(Candidate { area, index: i, generation: 0 })
        })
        .collect();

    let mut remaining = n;
    while remaining > 4 {
        let Some(Reverse(worst)) = heap.pop() else { break };
        if worst.generation != generation[worst.index] {
            continue;
        }
        if worst.area >= epsilon {
            // Accepted pops come out in non-decreasing area, so nothing live is smaller.
            break;
        }

        let (before, after) = (prev[worst.index], next[worst.index]);
        alive[worst.index] = false;
        next[before] = after;
        prev[after] = before;
        remaining -= 1;

        for m in [before, after] {
            // The floor at the area just removed is what makes the detail slider
            // monotonic: without it a survivor can become *cheaper* than a point already
            // gone, and two settings produce point sets that are not nested — nudging the
            // slider brings back a headland the blunter setting had removed.
            let fresh = triangle_area(points[prev[m]], points[m], points[next[m]], aspect);
            generation[m] += 1;
            heap.push(Reverse(Candidate {
                area: fresh.max(worst.area),
                index: m,
                generation: generation[m],
            }));
        }
    }

    (0..n).filter(|i| alive[*i]).map(|i| points[i]).collect()
}

/// Simplify a whole coastline at a user-facing detail level.
///
/// `detail` runs `0.0` (blunt) to `1.0` (keep everything). It maps to `epsilon`
/// geometrically, because the useful range of areas spans several orders of magnitude and
/// a linear slider would spend nine tenths of its travel doing nothing visible.
///
/// Rings that simplify away to nothing — specks a few pixels across — are dropped, and
/// each surviving ring's `area` is recomputed from its new points.
pub fn simplify(rings: Vec<Ring>, detail: f64, aspect: f64) -> Vec<Ring> {
    let epsilon = epsilon_for(detail);
    let mut kept: Vec<Ring> = rings
        .into_iter()
        .filter_map(|r| {
            let points = simplify_ring(&r.points, epsilon, aspect);
            let area = Ring::signed_area(&points).abs();
            // A ring whose area is not a number is not a ring; dropping it here is what
            // keeps a NaN from reaching the substrate.
            if points.len() < 4 || !area.is_finite() || area < MIN_RING_AREA {
                return None;
            }
            Some(Ring { points, is_hole: r.is_hole, area })
        })
        .collect();
    // Areas moved, so the largest-first order the tracer handed us has to be re-established.
    kept.sort_by(|a, b| b.area.total_cmp(&a.area));
    kept
}

/// The detail slider's curve. `1.0` keeps everything; `0.0` is as blunt as it goes.
pub fn epsilon_for(detail: f64) -> f64 {
    // 1e-9 is well under one pixel on any sane raster; 1e-4 is a visible headland.
    let t = 1.0 - detail.clamp(0.0, 1.0);
    1e-9 * (1e-4f64 / 1e-9).powf(t)
}

/// Effective area of the triangle `a`–`b`–`c`, with the horizontal squash of normalized
/// coordinates undone before it is measured.
fn triangle_area(a: [f64; 2], b: [f64; 2], c: [f64; 2], aspect: f64) -> f64 {
    let (ux, uy) = ((a[0] - b[0]) * aspect, a[1] - b[1]);
    let (vx, vy) = ((c[0] - b[0]) * aspect, c[1] - b[1]);
    let area = (ux * vy - uy * vx).abs() / 2.0;
    // A point that cannot be measured is a point we must not drop, and infinity is the
    // only value that both sorts last and never falls under a threshold.
    if area.is_finite() { area } else { f64::INFINITY }
}

/// One heap entry: a point's effective area at the moment it was pushed.
#[derive(Clone, Copy)]
struct Candidate {
    area: f64,
    index: usize,
    generation: u32,
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // `total_cmp` rather than `partial_cmp`: an order total over every f64 is what
        // keeps the removal sequence, and so the coastline, identical on every machine.
        self.area
            .total_cmp(&other.area)
            .then(self.index.cmp(&other.index))
            .then(self.generation.cmp(&other.generation))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Candidate {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    const SQUARE: [[f64; 2]; 4] = [[0.2, 0.2], [0.8, 0.2], [0.8, 0.8], [0.2, 0.8]];

    /// A closed polygon with `fill` extra, exactly collinear points along every edge.
    fn padded(corners: &[[f64; 2]], fill: usize) -> Vec<[f64; 2]> {
        let mut pts = Vec::new();
        for (i, a) in corners.iter().enumerate() {
            let b = corners[(i + 1) % corners.len()];
            for k in 0..=fill {
                let t = k as f64 / (fill + 1) as f64;
                pts.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            }
        }
        pts
    }

    fn polygon(sides: usize, radius: f64) -> Vec<[f64; 2]> {
        (0..sides)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / sides as f64;
                [0.5 + radius * a.cos(), 0.5 + radius * a.sin()]
            })
            .collect()
    }

    /// A wobbly coastline: still simple, but with no two points equally significant.
    fn coastline(n: usize, wobble: f64, seed: u64) -> Vec<[f64; 2]> {
        let mut rng = Rng::new(seed);
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                let r = 0.30 + rng.range(-wobble, wobble);
                [0.5 + r * a.cos(), 0.5 + r * a.sin()]
            })
            .collect()
    }

    fn ring(points: Vec<[f64; 2]>, is_hole: bool) -> Ring {
        let area = Ring::signed_area(&points).abs();
        Ring { points, is_hole, area }
    }

    /// Is `small` an ordered subset of `whole`? Ring order is preserved by simplification,
    /// so a genuine nesting is a subsequence, not merely a set inclusion.
    fn is_subsequence(small: &[[f64; 2]], whole: &[[f64; 2]]) -> bool {
        let mut it = whole.iter();
        small.iter().all(|p| it.any(|q| q == p))
    }

    #[test]
    fn a_square_padded_with_collinear_points_collapses_to_its_four_corners() {
        let pts = padded(&SQUARE, 9);
        assert_eq!(pts.len(), 40);
        assert_eq!(simplify_ring(&pts, 1e-6, 1.0), SQUARE.to_vec());
    }

    #[test]
    fn a_convex_polygon_keeps_every_one_of_its_extreme_points() {
        let hexagon = polygon(6, 0.4);
        let pts = padded(&hexagon, 7);
        let out = simplify_ring(&pts, 1e-3, 1.0);
        assert_eq!(out.len(), 6, "the six corners are all far above the threshold");
        for v in &hexagon {
            assert!(out.iter().any(|p| p == v), "corner {v:?} was lost");
        }
    }

    #[test]
    fn the_ring_is_closed_so_a_spike_at_index_zero_survives() {
        // Index 0 is the tip of a long spit; every other point is on a small circle, so
        // the spit is the ring's one significant feature and it sits on the seam.
        let mut pts = polygon(40, 0.02);
        pts[0] = [0.95, 0.5];
        let out = simplify_ring(&pts, epsilon_for(0.0), 1.0);
        assert!(out.len() < pts.len(), "the circle should have been thinned");
        // Survivors come back in ring order, so index 0 surviving means it leads.
        assert_eq!(out[0], [0.95, 0.5], "the most significant point was at the seam");
    }

    #[test]
    fn a_point_at_index_zero_on_a_straight_run_goes_like_any_other() {
        // The failure this catches: treating the ring as an open polyline, which pins the
        // first and last points and leaves a wart wherever the tracer happened to start.
        let mut pts = padded(&SQUARE, 9);
        pts.rotate_left(3);
        let first = pts[0];
        let last = pts[pts.len() - 1];

        let out = simplify_ring(&pts, 1e-6, 1.0);
        assert_eq!(out.len(), 4);
        assert!(!out.contains(&first), "the seam's first point is not a corner");
        assert!(!out.contains(&last), "the seam's last point is not a corner");
        for c in SQUARE {
            assert!(out.contains(&c));
        }
    }

    #[test]
    fn a_blunter_detail_keeps_a_subset_of_what_a_finer_one_kept() {
        let pts = coastline(400, 0.03, 812);
        let steps: Vec<Vec<[f64; 2]>> = [1e-8, 1e-7, 3e-7, 1e-6, 1e-5, 1e-4]
            .iter()
            .map(|e| simplify_ring(&pts, *e, 1.4))
            .collect();

        for pair in steps.windows(2) {
            let (fine, blunt) = (&pair[0], &pair[1]);
            assert!(
                blunt.len() <= fine.len(),
                "{} points survived a blunter cut than {}",
                blunt.len(),
                fine.len()
            );
            assert!(is_subsequence(blunt, fine), "the detail slider is not monotonic");
        }
        assert!(
            steps[0].len() > steps[steps.len() - 1].len(),
            "the sweep must actually remove something"
        );
    }

    #[test]
    fn a_ring_never_falls_below_four_points() {
        // A speck: every triangle in it is orders of magnitude under the threshold.
        let pts = padded(&[[0.5, 0.5], [0.501, 0.5], [0.501, 0.501], [0.5, 0.501]], 6);
        assert_eq!(simplify_ring(&pts, epsilon_for(0.0), 1.0).len(), 4);
        assert_eq!(simplify_ring(&pts, 1e9, 1.0).len(), 4);
    }

    #[test]
    fn a_ring_of_four_points_or_fewer_comes_back_untouched() {
        assert_eq!(simplify_ring(&SQUARE, 1e9, 1.0), SQUARE.to_vec());
        let two = [[0.1, 0.1], [0.2, 0.2]];
        assert_eq!(simplify_ring(&two, 1e9, 1.0), two.to_vec());
        assert_eq!(simplify_ring(&[], 1.0, 1.0), Vec::<[f64; 2]>::new());
    }

    #[test]
    fn doubling_the_aspect_is_exactly_doubling_the_threshold() {
        // Every triangle scales by `aspect`, so the two must agree point for point —
        // which is what "a wide map does not simplify differently along x" means.
        let pts = coastline(300, 0.03, 5);
        let eps = 3e-5;
        assert_eq!(simplify_ring(&pts, eps * 2.0, 2.0), simplify_ring(&pts, eps, 1.0));
    }

    #[test]
    fn a_nonsense_threshold_leaves_the_ring_alone() {
        let pts = padded(&SQUARE, 9);
        assert_eq!(simplify_ring(&pts, f64::NAN, 1.0), pts);
    }

    #[test]
    fn full_detail_changes_almost_nothing_and_every_step_down_takes_more() {
        let coast = vec![ring(coastline(500, 0.004, 99), false)];
        let kept: Vec<usize> = [1.0, 0.75, 0.5, 0.25, 0.0]
            .iter()
            .map(|d| simplify(coast.clone(), *d, 1.0)[0].points.len())
            .collect();

        assert!(kept[0] >= 490, "full detail dropped {} of 500 points", 500 - kept[0]);
        assert!(kept.windows(2).all(|w| w[0] >= w[1]), "the slider went backwards: {kept:?}");
        assert!(kept[4] * 2 < kept[0], "the blunt end should cost at least half: {kept:?}");
    }

    #[test]
    fn no_detail_takes_a_ring_down_to_its_four_point_floor() {
        // Nothing in a padded square is above any threshold except its corners, and the
        // floor is what stops the last four going too.
        let out = simplify(vec![ring(padded(&SQUARE, 9), false)], 0.0, 1.0);
        assert_eq!(out[0].points.len(), 4);
    }

    #[test]
    fn specks_are_dropped_and_a_lake_stays_a_lake() {
        let coast = vec![
            ring(coastline(200, 0.03, 3), false),
            ring(polygon(30, 0.05), true),
            // 1e-4 across: under MIN_RING_AREA by three orders of magnitude.
            ring(padded(&[[0.4, 0.4], [0.4001, 0.4], [0.4001, 0.4001], [0.4, 0.4001]], 4), false),
        ];
        let out = simplify(coast, 0.5, 1.0);

        assert_eq!(out.len(), 2, "the speck should be gone and nothing else");
        assert!(!out[0].is_hole, "the mainland is largest");
        assert!(out[1].is_hole, "the lake kept its flag");
    }

    #[test]
    fn every_area_is_recomputed_and_the_rings_come_back_largest_first() {
        // A twelve-gon this small is entirely below the blunt threshold, so it collapses
        // to four points — and no quadrilateral drawn from its vertices holds more than
        // two thirds of a regular twelve-gon's area.
        let dodecagon = ring(polygon(12, 0.014), false);
        // A ring already at the four-point floor cannot move at all, so parking one just
        // under the twelve-gon guarantees the two change places.
        let side = (0.95 * dodecagon.area).sqrt();
        let square = ring(
            vec![[0.1, 0.1], [0.1 + side, 0.1], [0.1 + side, 0.1 + side], [0.1, 0.1 + side]],
            true,
        );
        assert!(square.area < dodecagon.area, "the square starts out the smaller of the two");

        let out = simplify(vec![dodecagon, square], 0.0, 1.0);
        assert_eq!(out.len(), 2);
        for r in &out {
            assert_eq!(r.area, Ring::signed_area(&r.points).abs(), "area was not recomputed");
        }
        assert!(out[0].area >= out[1].area, "rings must come back largest first");
        assert!(out[0].is_hole, "the shrunken ring should have been sorted behind the square");
        assert_eq!(out[1].points.len(), 4);
    }

    #[test]
    fn a_ring_of_twenty_thousand_points_thins_without_breaking_a_sweat() {
        // What a real marching-squares trace of a 2000x1400 raster hands this stage. The
        // work is iterative throughout, so depth is never a question.
        // The wobble is one pixel of a 1400-tall raster: marching squares jitters by
        // exactly that much, and thinning it away is the whole point of the stage.
        let pts = coastline(20_000, 1.0 / 1400.0, 42);
        let out = simplify_ring(&pts, epsilon_for(0.55), 2000.0 / 1400.0);
        assert!(out.len() > 4 && out.len() < pts.len() / 2, "kept {} of 20000", out.len());
        assert!(is_subsequence(&out, &pts));
    }

    #[test]
    fn the_same_ring_simplifies_the_same_way_every_time() {
        let pts = coastline(600, 0.03, 1);
        let once = simplify_ring(&pts, 4e-6, 1.7);
        assert_eq!(once, simplify_ring(&pts, 4e-6, 1.7));
        assert!(once.len() < pts.len() && once.len() > 4);
        assert!(once.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
    }
}
