//! Stage 5 — the cell substrate.
//!
//! Poisson-disc sites, Delaunay triangulation, Voronoi cells. Everything downstream —
//! height, climate, rivers, biomes — is computed on this graph rather than on pixels,
//! because a river needs neighbours and a raster only has eight of them.

use delaunator::{EMPTY, Point, Triangulation, next_halfedge, triangulate};
use serde::{Deserialize, Serialize};

use crate::mask::Mask;
use crate::rng::Rng;

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

/// Bridson's candidate count per active sample. Fewer leaves visible gaps; more only costs
/// time, since the packing density has already levelled off by thirty.
const CANDIDATES: usize = 30;

/// How far outside the map the bounding frame sits, in the unsquashed space the mesh is
/// built in. Large enough that every bisector between a real hull site and a frame site
/// falls outside the map, so clipping — not the frame — decides the shape of an edge cell.
const FRAME_MARGIN: f64 = 0.35;

/// Frame spacing, in sampling radii.
const FRAME_SPACING: f64 = 3.0;

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
///
/// Sampling, triangulation, Voronoi and clipping all happen in the *unsquashed* rectangle
/// `[0, aspect] × [0, 1]`; `x` is divided by `aspect` only on the way out. Meshing in
/// normalized space instead would shear every triangle on a non-square map.
pub fn substrate(target: usize, seed: u64, mask: &Mask) -> Substrate {
    // A degenerate aspect would poison every distance downstream, and NaN must never reach
    // the output; a square map is the only safe stand-in.
    let aspect = if mask.aspect.is_finite() && mask.aspect > 0.0 { mask.aspect } else { 1.0 };

    let (raw, radius) = poisson_disc(target.max(1), seed, aspect);
    let real = raw.len();
    let points: Vec<Point> = raw
        .iter()
        .chain(&frame_sites(aspect, radius))
        .map(|p| Point { x: p[0], y: p[1] })
        .collect();

    let tri = triangulate(&points);
    if tri.is_empty() {
        return Substrate {
            aspect,
            sites: Vec::new(),
            polygons: Vec::new(),
            neighbors: Vec::new(),
            is_land: Vec::new(),
        };
    }

    let centers: Vec<[f64; 2]> = (0..tri.len())
        .map(|t| {
            let v = |k: usize| {
                let p = &points[tri.triangles[3 * t + k]];
                [p.x, p.y]
            };
            circumcenter(v(0), v(1), v(2))
        })
        .collect();
    let inedges = incoming_halfedges(&tri, points.len());

    let mut sites = Vec::with_capacity(real);
    let mut polygons = Vec::with_capacity(real);
    // Old (real-only) index to output index; the frame and any slivered cell drop out here.
    let mut index = vec![u32::MAX; real];

    for (i, site) in raw.iter().enumerate() {
        let ring = cell_ring(i, &tri, &inedges, &centers)
            .unwrap_or_else(|| fan_ring(i, &tri, &centers, *site));
        let ring = close_ring(clip_to_box(ring, aspect));
        if ring.len() < 3 {
            continue;
        }
        index[i] = sites.len() as u32;
        sites.push([site[0] / aspect, site[1]]);
        polygons.push(wind(ring.into_iter().map(|p| [p[0] / aspect, p[1]]).collect()));
    }

    let mut neighbors = vec![Vec::new(); sites.len()];
    for (e, &a) in tri.triangles.iter().enumerate() {
        let b = tri.triangles[next_halfedge(e)];
        if a >= real || b >= real {
            continue;
        }
        let (a, b) = (index[a], index[b]);
        // Both directions every time: a hull edge is only walked once.
        if a != u32::MAX && b != u32::MAX && a != b {
            neighbors[a as usize].push(b);
            neighbors[b as usize].push(a);
        }
    }
    for n in &mut neighbors {
        n.sort_unstable();
        n.dedup();
    }

    let is_land = sites.iter().map(|s| mask.sample(*s)).collect();
    Substrate { aspect, sites, polygons, neighbors, is_land }
}

/// Bridson's algorithm over `[0, aspect] × [0, 1]`. Returns the sites and the radius used.
fn poisson_disc(target: usize, seed: u64, aspect: f64) -> (Vec<[f64; 2]>, f64) {
    // A Poisson-disc packing reaches roughly 0.7 of the hexagonal density, whose cell area
    // is r² √3 / 2, so `count ≈ 0.7 · area / (r² √3 / 2)`. Bridson at thirty candidates
    // comes in about 22% under that ideal — measured across targets and seeds, not derived
    // — and the radius is shrunk by the same factor to compensate.
    const DENSITY: f64 = 0.7 * 0.78;
    let radius = (2.0 * DENSITY * aspect / (target as f64 * f64::sqrt(3.0))).sqrt();
    // The classic grid size: at r/√2 no cell can hold two samples, so occupancy is a single
    // index rather than a list.
    let cell = radius / std::f64::consts::SQRT_2;
    let gw = (aspect / cell).ceil() as usize + 1;
    let gh = (1.0 / cell).ceil() as usize + 1;

    let mut grid = vec![usize::MAX; gw * gh];
    let mut sites: Vec<[f64; 2]> = Vec::with_capacity(target + target / 4 + 1);
    let mut active: Vec<usize> = Vec::new();
    let mut rng = Rng::new(seed);

    let place = |p: [f64; 2], sites: &mut Vec<[f64; 2]>, grid: &mut Vec<usize>| -> usize {
        let (gx, gy) = ((p[0] / cell) as usize, (p[1] / cell) as usize);
        grid[gy.min(gh - 1) * gw + gx.min(gw - 1)] = sites.len();
        sites.push(p);
        sites.len() - 1
    };

    let first = [rng.range(0.0, aspect), rng.f64()];
    active.push(place(first, &mut sites, &mut grid));

    while !active.is_empty() {
        let slot = rng.below(active.len());
        let origin = sites[active[slot]];
        let mut placed = false;

        for _ in 0..CANDIDATES {
            let theta = rng.range(0.0, std::f64::consts::TAU);
            // Uniform over the annulus [r, 2r) rather than over its radius, which would
            // crowd candidates against the inner ring.
            let d = rng.range(radius * radius, 4.0 * radius * radius).sqrt();
            let p = [origin[0] + d * theta.cos(), origin[1] + d * theta.sin()];

            if p[0] < 0.0 || p[0] >= aspect || p[1] < 0.0 || p[1] >= 1.0 {
                continue;
            }
            if crowded(p, &sites, &grid, gw, gh, cell, radius) {
                continue;
            }
            active.push(place(p, &mut sites, &mut grid));
            placed = true;
            break;
        }

        if !placed {
            active.swap_remove(slot);
        }
    }

    (sites, radius)
}

/// Is any existing sample within `radius` of `p`? At a grid pitch of r/√2 the two-cell
/// neighbourhood is a superset of the disc, so nothing further out can matter.
fn crowded(
    p: [f64; 2],
    sites: &[[f64; 2]],
    grid: &[usize],
    gw: usize,
    gh: usize,
    cell: f64,
    radius: f64,
) -> bool {
    let (gx, gy) = ((p[0] / cell) as usize, (p[1] / cell) as usize);
    let (x0, x1) = (gx.saturating_sub(2), (gx + 2).min(gw - 1));
    let (y0, y1) = (gy.saturating_sub(2), (gy + 2).min(gh - 1));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = grid[y * gw + x];
            if i == usize::MAX {
                continue;
            }
            let (dx, dy) = (sites[i][0] - p[0], sites[i][1] - p[1]);
            if dx * dx + dy * dy < radius * radius {
                return true;
            }
        }
    }
    false
}

/// A rectangular ring of sites outside the map. They exist only to put every real site
/// strictly inside the convex hull, which is what makes every real Voronoi cell bounded.
fn frame_sites(aspect: f64, radius: f64) -> Vec<[f64; 2]> {
    let (x0, x1) = (-FRAME_MARGIN, aspect + FRAME_MARGIN);
    let (y0, y1) = (-FRAME_MARGIN, 1.0 + FRAME_MARGIN);
    let step = FRAME_SPACING * radius;
    let runs = |lo: f64, hi: f64| ((hi - lo) / step).ceil().max(1.0) as usize;
    let (nx, ny) = (runs(x0, x1), runs(y0, y1));

    let mut pts = Vec::with_capacity(2 * (nx + ny) + 2);
    for i in 0..=nx {
        let x = x0 + (x1 - x0) * i as f64 / nx as f64;
        pts.push([x, y0]);
        pts.push([x, y1]);
    }
    // The corners already came from the horizontal runs.
    for j in 1..ny {
        let y = y0 + (y1 - y0) * j as f64 / ny as f64;
        pts.push([x0, y]);
        pts.push([x1, y]);
    }
    pts
}

/// For every point, one halfedge that ends at it — preferring a hull halfedge, so a walk
/// that cannot close at least starts at the boundary instead of the middle of the fan.
fn incoming_halfedges(tri: &Triangulation, points: usize) -> Vec<usize> {
    let mut inedges = vec![EMPTY; points];
    for (e, &twin) in tri.halfedges.iter().enumerate() {
        let p = tri.triangles[next_halfedge(e)];
        if twin == EMPTY || inedges[p] == EMPTY {
            inedges[p] = e;
        }
    }
    inedges
}

/// Walk the halfedges around `site`, collecting incident circumcenters in order. `None`
/// when the fan does not close — an unbounded hull cell, or a broken triangulation.
fn cell_ring(
    site: usize,
    tri: &Triangulation,
    inedges: &[usize],
    centers: &[[f64; 2]],
) -> Option<Vec<[f64; 2]>> {
    let start = inedges[site];
    if start == EMPTY {
        return None;
    }
    let mut ring = Vec::with_capacity(8);
    let mut e = start;
    loop {
        ring.push(centers[e / 3]);
        let out = next_halfedge(e);
        if tri.triangles[out] != site || ring.len() > tri.len() {
            return None;
        }
        e = tri.halfedges[out];
        if e == EMPTY {
            return None;
        }
        if e == start {
            return Some(ring);
        }
    }
}

/// Fallback for a fan that will not close: every incident circumcenter, sorted by angle
/// about the site. Convex cells are recovered exactly by this; it is only slower.
fn fan_ring(site: usize, tri: &Triangulation, centers: &[[f64; 2]], at: [f64; 2]) -> Vec<[f64; 2]> {
    let mut ring: Vec<[f64; 2]> = tri
        .triangles
        .chunks_exact(3)
        .enumerate()
        .filter(|(_, corners)| corners.contains(&site))
        .map(|(t, _)| centers[t])
        .collect();
    let angle = |p: &[f64; 2]| (p[1] - at[1]).atan2(p[0] - at[0]);
    ring.sort_by(|a, b| angle(a).total_cmp(&angle(b)));
    ring
}

/// The Voronoi vertex of a triangle. A near-collinear triangle has its circumcenter at
/// infinity; the centroid stands in, because an infinity here would spread through the
/// clip into the output.
fn circumcenter(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> [f64; 2] {
    let centroid = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0];
    let (bx, by) = (b[0] - a[0], b[1] - a[1]);
    let (cx, cy) = (c[0] - a[0], c[1] - a[1]);
    let (bl, cl) = (bx * bx + by * by, cx * cx + cy * cy);
    let d = 2.0 * (bx * cy - by * cx);

    // Both sides are areas, so the test is scale-free: a triangle this flat has no usable
    // circumcenter at f64 precision.
    if !d.is_finite() || d.abs() <= 1e-12 * (bl + cl) {
        return centroid;
    }
    let p = [a[0] + (cy * bl - by * cl) / d, a[1] + (bx * cl - cx * bl) / d];
    if p[0].is_finite() && p[1].is_finite() { p } else { centroid }
}

/// Sutherland–Hodgman against the four half-planes of `[0, aspect] × [0, 1]`. Voronoi
/// cells are convex, so the convex-clip restriction of the algorithm never bites.
fn clip_to_box(ring: Vec<[f64; 2]>, aspect: f64) -> Vec<[f64; 2]> {
    let mut ring = ring;
    for (axis, bound, keep_above) in
        [(0, 0.0, true), (0, aspect, false), (1, 0.0, true), (1, 1.0, false)]
    {
        if ring.len() < 3 {
            return Vec::new();
        }
        let inside = |p: &[f64; 2]| if keep_above { p[axis] >= bound } else { p[axis] <= bound };
        let mut kept = Vec::with_capacity(ring.len() + 2);
        let mut prev = ring[ring.len() - 1];
        let mut prev_in = inside(&prev);
        for &cur in &ring {
            let cur_in = inside(&cur);
            if cur_in != prev_in {
                kept.push(meet(prev, cur, axis, bound));
            }
            if cur_in {
                kept.push(cur);
            }
            (prev, prev_in) = (cur, cur_in);
        }
        ring = kept;
    }
    ring
}

/// Where `a`→`b` crosses `axis = bound`. The crossed coordinate is set to the bound
/// exactly, so a clipped cell divides back to exactly 1.0 rather than a hair over.
fn meet(a: [f64; 2], b: [f64; 2], axis: usize, bound: f64) -> [f64; 2] {
    let t = (bound - a[axis]) / (b[axis] - a[axis]);
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let other = 1 - axis;
    let mut p = [0.0; 2];
    p[axis] = bound;
    p[other] = a[other] + (b[other] - a[other]) * t;
    p
}

/// Drop the duplicate vertices clipping leaves behind when a cell only grazes a boundary.
fn close_ring(ring: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let same =
        |a: &[f64; 2], b: &[f64; 2]| (a[0] - b[0]).abs() <= 1e-12 && (a[1] - b[1]).abs() <= 1e-12;
    let mut ring = ring;
    ring.dedup_by(|a, b| same(a, b));
    while ring.len() > 1 && same(&ring[0], &ring[ring.len() - 1]) {
        ring.pop();
    }
    ring
}

/// One winding for every cell. Positive shoelace area — which reads clockwise on screen,
/// since y grows southward.
fn wind(ring: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let mut ring = ring;
    if signed_area(&ring) < 0.0 {
        ring.reverse();
    }
    ring
}

fn signed_area(ring: &[[f64; 2]]) -> f64 {
    let mut sum = 0.0;
    for (i, a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        sum += a[0] * b[1] - b[0] * a[1];
    }
    sum / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mask of the given shape, land where `land` says so.
    fn mask_of(w: u32, h: u32, land: impl Fn(u32, u32) -> bool) -> Mask {
        let mut bits = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                bits.push(land(x, y));
            }
        }
        Mask { w, h, aspect: f64::from(w) / f64::from(h), land: bits }
    }

    fn all_land(w: u32, h: u32) -> Mask {
        mask_of(w, h, |_, _| true)
    }

    /// Crossing-number test, in whatever space the polygon is given in.
    fn contains(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
        let mut inside = false;
        for (i, a) in ring.iter().enumerate() {
            let b = ring[(i + 1) % ring.len()];
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                if p[0] < x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    #[test]
    fn the_same_seed_gives_the_same_mesh() {
        let m = all_land(400, 280);
        let a = substrate(600, 812, &m);
        let b = substrate(600, 812, &m);
        assert_eq!(a.sites, b.sites);
        assert_eq!(a.polygons, b.polygons);
        assert_eq!(a.neighbors, b.neighbors);
        assert_eq!(a.is_land, b.is_land);

        let c = substrate(600, 813, &m);
        assert_ne!(a.sites, c.sites, "a different seed is a different world");

        let n = a.len();
        assert!(n > 0);
        assert_eq!((a.polygons.len(), a.neighbors.len(), a.is_land.len()), (n, n, n));
    }

    #[test]
    fn the_cell_count_lands_near_the_target() {
        // The contract is ±25%; the calibrated radius holds ±5% from a few hundred cells
        // up, and this asserts the tighter figure so a regression in the constant shows.
        for (target, w, h) in [(300, 200, 200), (600, 400, 280), (1200, 500, 350), (2000, 800, 400)]
        {
            for seed in [1, 7, 99] {
                let n = substrate(target, seed, &all_land(w, h)).len();
                let off = (n as f64 / target as f64 - 1.0).abs();
                assert!(
                    off < 0.05,
                    "target {target} produced {n} cells, off by {:.1}%",
                    off * 100.0
                );
            }
        }
        let big = substrate(3000, 1, &all_land(2000, 1400)).len();
        assert!((2850..3150).contains(&big), "the map-sized case produced {big} cells");
    }

    #[test]
    fn every_site_lies_inside_its_own_polygon() {
        let s = substrate(800, 3, &all_land(500, 350));
        for i in 0..s.len() {
            assert!(
                contains(&s.polygons[i], s.sites[i]),
                "site {i} at {:?} is outside its own cell {:?}",
                s.sites[i],
                s.polygons[i]
            );
        }
    }

    #[test]
    fn no_polygon_pokes_outside_the_unit_square() {
        let s = substrate(800, 11, &all_land(600, 300));
        for ring in &s.polygons {
            assert!(ring.len() >= 3, "a cell survived with {} vertices", ring.len());
            for p in ring {
                assert!(
                    (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1]),
                    "vertex {p:?} escaped the map"
                );
            }
        }
        for site in &s.sites {
            assert!((0.0..=1.0).contains(&site[0]) && (0.0..=1.0).contains(&site[1]));
        }
    }

    #[test]
    fn adjacency_is_symmetric_and_irreflexive() {
        let s = substrate(700, 5, &all_land(400, 400));
        for (a, ns) in s.neighbors.iter().enumerate() {
            let a = a as u32;
            assert!(!ns.contains(&a), "cell {a} is its own neighbour");
            let mut sorted = ns.clone();
            sorted.dedup();
            assert_eq!(&sorted, ns, "cell {a} lists a neighbour twice");
            for &b in ns {
                assert!((b as usize) < s.len(), "cell {a} points at frame index {b}");
                assert!(
                    s.neighbors[b as usize].contains(&a),
                    "{a} knows {b} but {b} does not know {a}"
                );
            }
        }
        assert!(s.neighbors.iter().all(|n| !n.is_empty()), "an isolated cell would strand a river");
    }

    #[test]
    fn no_coordinate_is_nan_or_infinite() {
        // A one-pixel-tall map is the shape most likely to fold a circumcenter to infinity.
        for m in [all_land(500, 350), all_land(64, 1), all_land(1, 64)] {
            let s = substrate(400, 2, &m);
            assert!(s.aspect.is_finite() && s.aspect > 0.0);
            for p in s.sites.iter().chain(s.polygons.iter().flatten()) {
                assert!(p[0].is_finite() && p[1].is_finite(), "{p:?} is not a number");
            }
        }
    }

    #[test]
    fn a_degenerate_aspect_never_reaches_the_output() {
        let mut m = all_land(64, 64);
        m.aspect = f64::NAN;
        let s = substrate(200, 1, &m);
        assert_eq!(s.aspect, 1.0);
        assert!(!s.is_empty());
    }

    #[test]
    fn a_half_land_mask_marks_about_half_the_cells_as_land() {
        // Land is the northern half: y grows southward, so the land is y < 0.5.
        let m = mask_of(400, 400, |_, y| y < 200);
        let s = substrate(900, 4, &m);
        let land = s.is_land.iter().filter(|l| **l).count() as f64 / s.len() as f64;
        assert!((land - 0.5).abs() < 0.05, "half a map of land gave {land:.3}");

        for (i, site) in s.sites.iter().enumerate() {
            assert_eq!(s.is_land[i], site[1] < 0.5, "cell {i} at {site:?} is on the wrong side");
        }
    }

    #[test]
    fn a_wide_map_does_not_stretch_its_cells() {
        let s = substrate(1200, 9, &all_land(2000, 1000));
        let (mut wide, mut tall) = (0.0, 0.0);
        for ring in &s.polygons {
            let x = |f: fn(f64, f64) -> f64| ring.iter().map(|p| p[0]).fold(ring[0][0], f);
            let y = |f: fn(f64, f64) -> f64| ring.iter().map(|p| p[1]).fold(ring[0][1], f);
            // Undo the squash before comparing: the cells are round in this space, not in
            // normalized coordinates.
            wide += (x(f64::max) - x(f64::min)) * s.aspect;
            tall += y(f64::max) - y(f64::min);
        }
        let ratio = wide / tall;
        assert!((ratio - 1.0).abs() < 0.1, "cells average {ratio:.2}× wider than tall");
    }

    #[test]
    fn every_polygon_is_wound_the_same_way() {
        let s = substrate(600, 6, &all_land(500, 300));
        for (i, ring) in s.polygons.iter().enumerate() {
            assert!(signed_area(ring) > 0.0, "cell {i} winds the other way: {ring:?}");
        }
    }

    #[test]
    fn poisson_sites_keep_their_distance() {
        let aspect = 1.5;
        let (sites, r) = poisson_disc(500, 21, aspect);
        for (i, a) in sites.iter().enumerate() {
            for b in &sites[i + 1..] {
                let d = f64::hypot(a[0] - b[0], a[1] - b[1]);
                assert!(d >= r * (1.0 - 1e-9), "two sites are {d} apart, closer than {r}");
            }
            assert!((0.0..aspect).contains(&a[0]) && (0.0..1.0).contains(&a[1]));
        }
    }

    #[test]
    fn the_frame_encloses_every_real_site() {
        let (x0, x1) = (-FRAME_MARGIN, 1.7 + FRAME_MARGIN);
        let f = frame_sites(1.7, 0.05);
        assert!(f.iter().any(|p| p[0] == x0 && p[1] == -FRAME_MARGIN), "no corner at the origin");
        assert!(f.iter().all(|p| {
            p[0] <= x0 + 1e-12 || p[0] >= x1 - 1e-12 || p[1] <= -FRAME_MARGIN || p[1] >= 1.0
        }));
        // Nothing inside the map: a frame site would take a cell away from a real one.
        assert!(!f.iter().any(|p| (0.0..=1.7).contains(&p[0]) && (0.0..=1.0).contains(&p[1])));
    }

    #[test]
    fn the_halfedge_walk_closes_for_every_real_site_and_the_fallback_agrees_with_it() {
        let (raw, radius) = poisson_disc(300, 42, 1.4);
        let points: Vec<Point> = raw
            .iter()
            .chain(&frame_sites(1.4, radius))
            .map(|p| Point { x: p[0], y: p[1] })
            .collect();
        let tri = triangulate(&points);
        let centers: Vec<[f64; 2]> = (0..tri.len())
            .map(|t| {
                let v = |k: usize| {
                    let p = &points[tri.triangles[3 * t + k]];
                    [p.x, p.y]
                };
                circumcenter(v(0), v(1), v(2))
            })
            .collect();
        let inedges = incoming_halfedges(&tri, points.len());

        for (i, site) in raw.iter().enumerate() {
            // This is the whole point of the frame: no real site is on the hull, so no fan
            // is open and no cell needs a special case downstream.
            let walked = cell_ring(i, &tri, &inedges, &centers).expect("cell {i} did not close");
            let fanned = fan_ring(i, &tri, &centers, *site);
            assert_eq!(walked.len(), fanned.len(), "cell {i} lost a vertex one way or the other");
            let (a, b) = (signed_area(&walked).abs(), signed_area(&fanned).abs());
            assert!((a - b).abs() < 1e-12, "cell {i}: walk gave area {a}, angle sort gave {b}");
        }
    }

    #[test]
    fn a_flat_triangle_yields_a_point_rather_than_an_infinity() {
        let c = circumcenter([0.0, 0.0], [1.0, 0.0], [2.0, 0.0]);
        assert!(c[0].is_finite() && c[1].is_finite(), "collinear points produced {c:?}");
        // The honest case still has to be right: the circumcentre of the unit right
        // triangle is the midpoint of its hypotenuse.
        let c = circumcenter([0.0, 0.0], [2.0, 0.0], [0.0, 2.0]);
        assert!((c[0] - 1.0).abs() < 1e-12 && (c[1] - 1.0).abs() < 1e-12, "{c:?}");
    }

    #[test]
    fn clipping_trims_a_polygon_to_the_box_and_leaves_an_inner_one_alone() {
        let inner = vec![[0.2, 0.2], [0.8, 0.2], [0.8, 0.8], [0.2, 0.8]];
        assert_eq!(clip_to_box(inner.clone(), 1.0), inner);

        let straddling = vec![[-0.5, 0.5], [0.5, -0.5], [1.5, 0.5], [0.5, 1.5]];
        let cut = clip_to_box(straddling, 1.0);
        assert!(cut.len() >= 3);
        for p in &cut {
            assert!((0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1]), "{p:?}");
        }
        assert!(contains(&cut, [0.5, 0.5]), "clipping lost the middle");

        let outside = vec![[2.0, 2.0], [3.0, 2.0], [3.0, 3.0]];
        assert!(clip_to_box(outside, 1.0).len() < 3, "a cell outside the map must vanish");
    }

    #[test]
    fn a_map_wider_than_it_is_tall_is_meshed_without_shearing() {
        // The regression this guards: triangulating in normalized space, which packs the
        // sites into columns and makes every cell a vertical sliver.
        let s = substrate(1000, 13, &all_land(2000, 1400));
        assert!(s.len() > 700, "only {} cells", s.len());
        let mean = s.neighbors.iter().map(Vec::len).sum::<usize>() as f64 / s.len() as f64;
        // A planar triangulation averages under six neighbours; a sheared one collapses
        // toward four.
        assert!((5.0..6.5).contains(&mean), "mean degree {mean:.2} is not a Delaunay mesh");
    }
}
