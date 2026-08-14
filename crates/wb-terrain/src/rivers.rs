//! Stage 8 — drainage.
//!
//! Priority-flood to make every basin drain, steepest descent to pick a course, flow
//! accumulation to decide which courses are big enough to be rivers.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

use crate::cells::Substrate;
use crate::params::RiverParams;

/// A single watercourse, from where it becomes a river to where it ends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct River {
    /// The cells it runs through, upstream first.
    pub cells: Vec<u32>,
    /// The same course as a polyline in normalized coordinates. Separate from `cells`
    /// because rendering wants a smooth line, not a string of Voronoi centres.
    pub points: Vec<[f64; 2]>,
    /// Accumulated flux at each point, so the channel can widen downstream.
    pub flux: Vec<f32>,
    /// Strahler number. Stroke weight, and a cheap way to label the main stem.
    pub order: u32,
    /// The cell this river empties into, and what is there.
    pub mouth: Mouth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mouth {
    Sea,
    Lake,
    /// A basin with no outlet — the river simply ends. Real, and worth drawing honestly
    /// rather than routing to the nearest coast.
    Endorheic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiverNetwork {
    pub rivers: Vec<River>,
    /// Accumulated flux per cell, normalized so `1.0` is the map's total rainfall.
    pub flux: Vec<f32>,
    /// Per cell: filled by the depression pass, i.e. under water.
    pub lake: Vec<bool>,
    /// Where each land cell drains. `None` at the sea, and in a basin with no outlet.
    pub downhill: Vec<Option<u32>>,
}

/// A fill this shallow is the arithmetic of the flood, not a lake.
const FILL_EPSILON: f32 = 1e-4;

/// How near the unit square a Voronoi vertex has to sit for its cell to count as running
/// off the map. Clipping puts them exactly on the border, so this only absorbs rounding.
const EDGE_EPSILON: f64 = 1e-9;

/// Work out where the water goes.
///
/// Requirements, in order:
///
/// 1. **Priority-flood.** Push every sea-adjacent land cell into a min-heap keyed on
///    height; pop the lowest, and raise each unvisited neighbour to at least the popped
///    height before pushing it. The result is a *filled* height field in which every land
///    cell has a downhill path to the sea. Keep it separate from the real heights — the
///    map must still render the valley, not the lake that filled it.
/// 2. **Lakes** are cells where the fill raised the height by more than a hair. A fill
///    deeper than `max_lake_depth` is a genuine closed basin instead: leave those cells
///    unfilled and dry, and rivers entering them end as [`Mouth::Endorheic`].
/// 3. **Downhill** is the steepest descent on the filled field, by gradient — height drop
///    over [`Substrate::distance`], not raw drop, or big cells win every time.
/// 4. **Flux.** Each cell starts with its own `precipitation`. Process cells in order of
///    descending filled height so every cell is done before the one it drains into, and
///    add each cell's total to its downhill neighbour. Normalize by the map's total
///    rainfall, so `threshold` means the same thing on every map.
/// 5. **Rivers.** A river starts at the first cell where flux crosses `threshold` and
///    follows `downhill` until it reaches sea, a lake, or a closed basin. Cells already
///    claimed by a river belong to that river — a tributary stops where it joins, and
///    the joined river's flux is already the sum. Compute the Strahler order from the
///    junction structure.
/// 6. `points` are the sites of `cells`, extended at the mouth to the shoreline midpoint
///    between the last land cell and the water it empties into, so a river does not stop
///    short of its own coast.
///
/// Three details the steps leave open, resolved here:
///
/// - Land that no sea cell borders — an island whose whole rim runs off the map — is
///   flooded from its own lowest cell, and water that reaches the map edge leaves the map
///   rather than pooling against it.
/// - A lake surface is flat, so step 3 finds no gradient across it. Those cells follow the
///   order the flood reached them in, which runs from the outlet inward.
/// - The total rainfall of step 4 is the rainfall over land: only that water enters the
///   network, so only that makes `threshold` independent of how much of the map is ocean.
pub fn rivers(
    sub: &Substrate,
    heights: &[f32],
    precipitation: &[f32],
    sea_level: f32,
    p: &RiverParams,
) -> RiverNetwork {
    let _ = sea_level; // `is_land` already encodes it, and is the invariant stage 6 promises.
    let n = sub.len();
    let edge: Vec<bool> =
        (0..n).map(|i| sub.polygons.get(i).is_some_and(|poly| touches_border(poly))).collect();

    let mut flood = Flood::new(sub, heights);
    flood.run(&edge);
    let Flood { mut filled, parent, discovery, .. } = flood;

    // 2. What the fill left standing is a lake; what it drowned too deep is a basin.
    let mut lake = vec![false; n];
    let mut closed = vec![false; n];
    for i in 0..n {
        if !sub.is_land[i] {
            continue;
        }
        let excess = filled[i] - heights[i];
        if excess <= FILL_EPSILON {
            continue;
        }
        if excess > p.max_lake_depth {
            filled[i] = heights[i];
            closed[i] = true;
        } else {
            lake[i] = true;
        }
    }

    // 3. Steepest descent by gradient.
    let mut downhill: Vec<Option<u32>> = vec![None; n];
    for a in 0..n {
        if !sub.is_land[a] {
            continue;
        }
        let mut best: Option<u32> = None;
        let mut steepest = f64::NEG_INFINITY;
        for &b in &sub.neighbors[a] {
            let j = b as usize;
            if filled[j] >= filled[a] {
                continue;
            }
            let d = sub.distance(a, j);
            if d <= 0.0 {
                continue;
            }
            let gradient = f64::from(filled[a] - filled[j]) / d;
            if gradient > steepest || (gradient == steepest && best.is_some_and(|c| b < c)) {
                steepest = gradient;
                best = Some(b);
            }
        }
        // Flat water has no steepest descent, so fall back to where the flood came from.
        // A reverted basin is dry ground again and gets no such help: its lowest cell is
        // meant to be a dead end. The parent forest is acyclic and never rises, which is
        // what keeps the chains below finite.
        downhill[a] =
            best.or_else(|| parent[a].filter(|&q| !closed[a] && filled[q as usize] <= filled[a]));
    }

    // 4. Flow accumulation. `f32::max` yields the other operand for a NaN, so a NaN in the
    // climate field lands as zero rain rather than poisoning every cell downstream.
    let mut flux: Vec<f32> =
        (0..n).map(|i| if sub.is_land[i] { precipitation[i].max(0.0) } else { 0.0 }).collect();
    let total: f32 = flux.iter().sum();

    // Descending filled height puts every cell before the one it drains into. Equal heights
    // are a lake surface, where the only downhill edges are flood-parent edges, so falling
    // back to reverse discovery order keeps that guarantee across the flat.
    let mut order: Vec<u32> = (0..n as u32).filter(|&i| sub.is_land[i as usize]).collect();
    order.sort_by(|&a, &b| {
        let (x, y) = (a as usize, b as usize);
        filled[y].total_cmp(&filled[x]).then(discovery[y].cmp(&discovery[x])).then(a.cmp(&b))
    });
    for &c in &order {
        if let Some(d) = downhill[c as usize] {
            let add = flux[c as usize];
            flux[d as usize] += add;
        }
    }
    if total > 0.0 {
        for f in &mut flux {
            *f /= total;
        }
    }
    for (f, &land) in flux.iter_mut().zip(&sub.is_land) {
        if !land {
            *f = 0.0;
        }
    }

    // 5. Strahler order, and the cells where a river begins.
    let channel: Vec<bool> =
        (0..n).map(|i| sub.is_land[i] && !lake[i] && flux[i] >= p.threshold).collect();
    let mut strahler = vec![0u32; n];
    let mut arriving = vec![0u32; n]; // highest order arriving, and how many at that order
    let mut arriving_count = vec![0u32; n];
    let mut sources: Vec<u32> = Vec::new();
    for &c in &order {
        let c = c as usize;
        if !channel[c] {
            continue;
        }
        if arriving_count[c] == 0 {
            sources.push(c as u32);
        }
        strahler[c] = if arriving_count[c] >= 2 { arriving[c] + 1 } else { arriving[c].max(1) };
        let Some(d) = downhill[c] else { continue };
        let d = d as usize;
        if !channel[d] {
            continue;
        }
        match strahler[c].cmp(&arriving[d]) {
            Ordering::Greater => {
                arriving[d] = strahler[c];
                arriving_count[d] = 1;
            }
            Ordering::Equal => arriving_count[d] += 1,
            Ordering::Less => {}
        }
    }

    // The biggest headwater claims its course first, so the main stem is the one that
    // reaches the mouth and everything else is a tributary of it.
    sources.sort_by(|&a, &b| flux[b as usize].total_cmp(&flux[a as usize]).then(a.cmp(&b)));

    let mut claimed: Vec<Option<u32>> = vec![None; n];
    let mut rivers: Vec<River> = Vec::new();
    for &s in &sources {
        let mut c = s as usize;
        if claimed[c].is_some() {
            continue;
        }
        let index = rivers.len() as u32;
        let mut cells: Vec<u32> = Vec::new();
        let (mouth, outlet) = loop {
            claimed[c] = Some(index);
            cells.push(c as u32);
            let Some(next) = downhill[c] else {
                // Water at the border has left the map; inland it has nowhere to go.
                break (if edge[c] { Mouth::Sea } else { Mouth::Endorheic }, None);
            };
            let d = next as usize;
            if !sub.is_land[d] {
                break (Mouth::Sea, Some(d));
            }
            if lake[d] {
                break (Mouth::Lake, Some(d));
            }
            match claimed[d] {
                // A confluence. A tributary's water ends wherever the river it joins ends.
                Some(other) if other != index => break (rivers[other as usize].mouth, Some(d)),
                // Only reachable if `downhill` cycles, which a correct fill rules out.
                // Stopping is worth four lines against the alternative.
                Some(_) => break (Mouth::Endorheic, None),
                None => {}
            }
            // Flux only grows downstream, so a course cannot run out of channel. If it
            // somehow does, end it here rather than draw a river that is not one.
            if !channel[d] {
                break (Mouth::Endorheic, None);
            }
            c = d;
        };

        // 6. Reach the coast: the shore lies about midway between the last land site and
        // the water it drains into. A confluence instead meets the trunk on its own line.
        let mut points: Vec<[f64; 2]> = cells.iter().map(|&i| sub.sites[i as usize]).collect();
        let mut widths: Vec<f32> = cells.iter().map(|&i| flux[i as usize]).collect();
        if let Some(d) = outlet {
            let (a, b) = (sub.sites[c], sub.sites[d]);
            points.push(if sub.is_land[d] && !lake[d] {
                b
            } else {
                [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]
            });
            widths.push(flux[c]);
        }

        rivers.push(River { cells, points, flux: widths, order: strahler[c], mouth });
    }

    RiverNetwork { rivers, flux, lake, downhill }
}

/// Does this cell's polygon reach the border of the map?
fn touches_border(polygon: &[[f64; 2]]) -> bool {
    polygon.iter().any(|v| {
        v[0] <= EDGE_EPSILON
            || v[0] >= 1.0 - EDGE_EPSILON
            || v[1] <= EDGE_EPSILON
            || v[1] >= 1.0 - EDGE_EPSILON
    })
}

/// A heap entry. `f32` is not `Ord`, and ties have to break the same way on every machine
/// or the filled field — and so the whole terrain — depends on the allocator.
struct Step(f32, u32);

impl Ord for Step {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0).then(self.1.cmp(&other.1))
    }
}

impl PartialOrd for Step {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Step {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Step {}

/// The priority-flood, mid-flight. A struct rather than a long argument list because the
/// drain loop has to be restarted for land the sea never reached.
struct Flood<'a> {
    sub: &'a Substrate,
    heights: &'a [f32],
    filled: Vec<f32>,
    /// The cell the flood arrived from. Rooted at the outlets, so it always leads out.
    parent: Vec<Option<u32>>,
    /// The rank each cell was reached in. The only ordering available across a flat lake.
    discovery: Vec<u32>,
    visited: Vec<bool>,
    seen: u32,
    heap: BinaryHeap<Reverse<Step>>,
}

impl<'a> Flood<'a> {
    fn new(sub: &'a Substrate, heights: &'a [f32]) -> Self {
        let n = sub.len();
        Self {
            sub,
            heights,
            filled: heights.to_vec(),
            parent: vec![None; n],
            discovery: vec![0; n],
            // The sea is where the water goes, not something to route through.
            visited: sub.is_land.iter().map(|&land| !land).collect(),
            seen: 0,
            heap: BinaryHeap::new(),
        }
    }

    fn run(&mut self, edge: &[bool]) {
        let sub = self.sub;
        for (i, &off_map) in edge.iter().enumerate() {
            let coastal = || sub.neighbors[i].iter().any(|&j| !sub.is_land[j as usize]);
            if sub.is_land[i] && (off_map || coastal()) {
                self.seed(i);
            }
        }
        self.drain();

        // Land with neither a sea neighbour nor a border still has to drain somewhere, so
        // start it at its own lowest cell and let the basin form around that.
        let mut stranded: Vec<u32> =
            (0..sub.len() as u32).filter(|&i| !self.visited[i as usize]).collect();
        stranded.sort_by(|&a, &b| {
            Step(self.heights[a as usize], a).cmp(&Step(self.heights[b as usize], b))
        });
        for c in stranded {
            if self.visited[c as usize] {
                continue;
            }
            self.seed(c as usize);
            self.drain();
        }
    }

    fn seed(&mut self, c: usize) {
        self.visited[c] = true;
        self.seen += 1;
        self.discovery[c] = self.seen;
        self.heap.push(Reverse(Step(self.filled[c], c as u32)));
    }

    /// Iterative on purpose: a recursive fill overflows the stack on a real map.
    fn drain(&mut self) {
        let (sub, heights) = (self.sub, self.heights);
        while let Some(Reverse(Step(h, c))) = self.heap.pop() {
            for &nb in &sub.neighbors[c as usize] {
                let j = nb as usize;
                if self.visited[j] {
                    continue;
                }
                self.visited[j] = true;
                self.filled[j] = heights[j].max(h);
                self.parent[j] = Some(c);
                self.seen += 1;
                self.discovery[j] = self.seen;
                self.heap.push(Reverse(Step(self.filled[j], nb)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const SEA_LEVEL: f32 = 0.32;

    /// Token polygons well inside the unit square, so no cell counts as running off the
    /// map unless a test says otherwise.
    fn assemble(sites: Vec<[f64; 2]>, neighbors: Vec<Vec<u32>>, heights: &[f32]) -> Substrate {
        let polygons = sites
            .iter()
            .map(|s| {
                let r = 0.002;
                vec![[s[0] - r, s[1] - r], [s[0] + r, s[1] - r], [s[0] + r, s[1] + r]]
            })
            .collect();
        // The invariant stage 6 promises, so no test can drift from it by hand.
        let is_land = heights.iter().map(|h| *h > SEA_LEVEL).collect();
        Substrate { aspect: 1.0, sites, polygons, neighbors, is_land }
    }

    /// A `w × h` grid, four-connected, row-major. Cell `(x, y)` is index `y * w + x`, and
    /// `y` grows southward as everywhere else in the crate.
    fn grid(w: usize, h: usize, heights: &[f32]) -> Substrate {
        let mut sites = Vec::new();
        let mut neighbors = Vec::new();
        for y in 0..h {
            for x in 0..w {
                sites.push([(x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / h as f64]);
                let mut nb = Vec::new();
                if x > 0 {
                    nb.push((y * w + x - 1) as u32);
                }
                if x + 1 < w {
                    nb.push((y * w + x + 1) as u32);
                }
                if y > 0 {
                    nb.push(((y - 1) * w + x) as u32);
                }
                if y + 1 < h {
                    nb.push(((y + 1) * w + x) as u32);
                }
                neighbors.push(nb);
            }
        }
        assemble(sites, neighbors, heights)
    }

    /// An explicit mesh: sites plus undirected edges.
    fn mesh(sites: &[[f64; 2]], edges: &[(usize, usize)], heights: &[f32]) -> Substrate {
        let mut neighbors = vec![Vec::new(); sites.len()];
        for &(a, b) in edges {
            neighbors[a].push(b as u32);
            neighbors[b].push(a as u32);
        }
        assemble(sites.to_vec(), neighbors, heights)
    }

    /// A plane falling southward, five columns wide, with a row of sea along the bottom.
    fn plane() -> Vec<f32> {
        let mut h = Vec::new();
        for y in 0..5 {
            h.extend([0.9 - 0.1 * y as f32; 5]);
        }
        h.extend([0.2; 5]);
        h
    }

    /// Four cells in a line: a headwater, a shoulder, a hollow, and a rim cell on the sea.
    fn valley(hollow: f32) -> (Substrate, Vec<f32>) {
        let heights = vec![0.9, 0.88, hollow, 0.85, 0.2];
        let sites = [[0.5, 0.1], [0.5, 0.3], [0.5, 0.5], [0.5, 0.7], [0.5, 0.9]];
        let edges = [(0, 1), (1, 2), (2, 3), (3, 4)];
        (mesh(&sites, &edges, &heights), heights)
    }

    /// Two equal headwaters joining above a single outlet to the sea.
    fn confluence() -> (Substrate, Vec<f32>) {
        let heights = vec![0.8, 0.8, 0.6, 0.5, 0.2];
        let sites = [[0.3, 0.2], [0.5, 0.2], [0.4, 0.4], [0.4, 0.6], [0.4, 0.8]];
        let edges = [(0, 1), (0, 2), (1, 2), (2, 3), (3, 4)];
        (mesh(&sites, &edges, &heights), heights)
    }

    fn params(threshold: f32) -> RiverParams {
        RiverParams { threshold, ..RiverParams::default() }
    }

    /// Uniform rain, so every flux is a plain fraction of the cell count.
    fn run(sub: &Substrate, heights: &[f32], threshold: f32) -> RiverNetwork {
        let rain = vec![1.0; sub.len()];
        rivers(sub, heights, &rain, SEA_LEVEL, &params(threshold))
    }

    /// Walk `downhill` from `start`, refusing to visit a cell twice.
    fn chain(net: &RiverNetwork, start: usize) -> Vec<usize> {
        let mut seen = BTreeSet::new();
        let mut path = vec![start];
        let mut c = start;
        while let Some(d) = net.downhill[c] {
            assert!(seen.insert(c), "downhill revisited cell {c} from {start}");
            c = d as usize;
            path.push(c);
        }
        path
    }

    #[test]
    fn an_inclined_plane_drains_every_land_cell_to_the_sea() {
        let heights = plane();
        let sub = grid(5, 6, &heights);
        let net = run(&sub, &heights, 0.05);

        assert!(net.lake.iter().all(|l| !l), "a plane has nothing to fill");
        for c in 0..25 {
            let path = chain(&net, c);
            let last = *path.last().unwrap();
            assert!(!sub.is_land[last], "cell {c} ended on land at {last}");
            assert_eq!(last / 5, 5, "cell {c} did not reach the sea row");
        }
        // Twenty-five land cells of equal rain: the bottom of a column carries five of them.
        assert!((net.flux[20] - 0.2).abs() < 1e-6, "{}", net.flux[20]);
        assert!((net.flux[0] - 0.04).abs() < 1e-6, "{}", net.flux[0]);
    }

    #[test]
    fn a_river_on_a_plane_runs_straight_down_the_steepest_line() {
        let heights = plane();
        let sub = grid(5, 6, &heights);
        let net = run(&sub, &heights, 0.05);

        assert_eq!(net.rivers.len(), 5, "one river per column of the plane");
        for r in &net.rivers {
            let column = r.cells[0] % 5;
            assert!(r.cells.iter().all(|c| c % 5 == column), "{:?} wandered sideways", r.cells);
            assert!(r.cells.windows(2).all(|w| w[1] == w[0] + 5), "{:?} is not a descent", r.cells);
            assert_eq!(r.mouth, Mouth::Sea);
            assert_eq!(r.order, 1, "nothing joins it");
            // The row-0 cell is below threshold, so the course is rows 1..=4 plus the shore.
            assert_eq!(r.cells.len(), 4);
            assert_eq!(r.points.len(), 5);
            assert_eq!(r.flux.len(), r.points.len());
            let shore = *r.points.last().unwrap();
            let expected = (4.5 / 6.0 + 5.5 / 6.0) / 2.0;
            assert!((shore[1] - expected).abs() < 1e-9, "mouth stopped short: {shore:?}");
        }
    }

    #[test]
    fn a_bowl_in_the_plane_becomes_a_lake_with_an_outlet() {
        let mut heights = plane();
        heights[2 + 2 * 5] = 0.45; // a hollow two rows down the slope
        let sub = grid(5, 6, &heights);
        let net = run(&sub, &heights, 0.05);

        assert_eq!(net.lake.iter().filter(|l| **l).count(), 1, "only the hollow is under water");
        assert!(net.lake[12]);
        // The rim it fills to is the cell below it, and the water carries on from there.
        assert_eq!(net.downhill[12], Some(17));
        let path = chain(&net, 12);
        assert!(!sub.is_land[*path.last().unwrap()], "the lake has no outlet: {path:?}");
    }

    #[test]
    fn a_shallow_hollow_fills_and_passes_the_water_on() {
        let (sub, heights) = valley(0.75);
        let net = run(&sub, &heights, 0.1);

        assert_eq!(net.lake, vec![false, false, true, false, false]);
        assert_eq!(net.downhill[2], Some(3), "the lake drains over its rim");
        assert_eq!(chain(&net, 0), vec![0, 1, 2, 3, 4]);
        let arriving = net.rivers.iter().find(|r| r.cells.contains(&0)).unwrap();
        assert_eq!(arriving.mouth, Mouth::Lake);
        assert_eq!(arriving.cells, vec![0, 1], "a river stops at the water's edge");
    }

    #[test]
    fn a_basin_past_the_depth_limit_ends_a_river_instead_of_flooding() {
        // The same hollow, deep enough that filling it would drown half a mountain.
        let (sub, heights) = valley(0.4);
        let net = run(&sub, &heights, 0.1);

        assert!(net.lake.iter().all(|l| !l), "0.45 of fill is a basin, not a lake");
        assert_eq!(net.downhill[2], None, "the basin bottom is a dead end");
        let dying = net.rivers.iter().find(|r| r.cells.contains(&0)).unwrap();
        assert_eq!(dying.mouth, Mouth::Endorheic);
        assert_eq!(dying.cells, vec![0, 1, 2]);
        assert_eq!(dying.points.len(), 3, "nothing to reach for: no shore point");
    }

    #[test]
    fn flux_at_a_confluence_is_the_sum_of_what_arrives() {
        let (sub, heights) = confluence();
        let net = run(&sub, &heights, 0.1);

        // Four land cells of equal rain, so each contributes a quarter.
        assert!((net.flux[0] - 0.25).abs() < 1e-6, "{}", net.flux[0]);
        assert!((net.flux[1] - 0.25).abs() < 1e-6, "{}", net.flux[1]);
        let junction = net.flux[0] + net.flux[1] + 0.25;
        assert!((net.flux[2] - junction).abs() < 1e-6, "{} != {junction}", net.flux[2]);
        assert!((net.flux[3] - 1.0).abs() < 1e-6, "the last land cell carries the whole map");
        assert_eq!(net.flux[4], 0.0, "the sea accumulates nothing");
    }

    #[test]
    fn two_streams_of_equal_order_make_an_order_two_river() {
        let (sub, heights) = confluence();
        let net = run(&sub, &heights, 0.1);

        assert_eq!(net.rivers.len(), 2);
        let stem = net.rivers.iter().find(|r| r.cells.contains(&3)).unwrap();
        let branch = net.rivers.iter().find(|r| !r.cells.contains(&3)).unwrap();
        assert_eq!(stem.order, 2, "two order-one streams meet on it");
        assert_eq!(branch.order, 1);
        assert_eq!(stem.mouth, Mouth::Sea);
        assert_eq!(branch.mouth, Mouth::Sea, "a tributary ends where its trunk does");
        // The tributary meets the trunk on the trunk's own line, not half a cell short.
        assert_eq!(*branch.points.last().unwrap(), sub.sites[2]);
    }

    #[test]
    fn raising_the_threshold_leaves_only_the_biggest_rivers() {
        let (sub, heights) = confluence();
        assert_eq!(run(&sub, &heights, 0.1).rivers.len(), 2);
        assert_eq!(run(&sub, &heights, 0.8).rivers.len(), 1, "only the trunk is left");
        assert_eq!(run(&sub, &heights, 1.5).rivers.len(), 0, "nothing carries that much");
    }

    #[test]
    fn the_sea_carries_no_flux_and_drains_nowhere() {
        let heights = plane();
        let sub = grid(5, 6, &heights);
        let net = run(&sub, &heights, 0.05);

        for c in 25..30 {
            assert_eq!(net.flux[c], 0.0);
            assert_eq!(net.downhill[c], None);
            assert!(!net.lake[c], "the sea is not a lake");
        }
    }

    #[test]
    fn water_reaching_the_map_edge_leaves_the_map() {
        let heights = [0.9, 0.7, 0.5];
        let sites = [[0.5, 0.5], [0.5, 0.7], [0.5, 0.9]];
        let mut sub = mesh(&sites, &[(0, 1), (1, 2)], &heights);
        sub.polygons[2] = vec![[0.48, 0.98], [0.52, 0.98], [0.5, 1.0]];
        let net = run(&sub, &heights, 0.1);

        assert!(net.lake.iter().all(|l| !l), "nothing pools against the border");
        assert_eq!(net.downhill[2], None, "there is no cell past the edge");
        assert_eq!(net.rivers.len(), 1);
        assert_eq!(net.rivers[0].mouth, Mouth::Sea, "off the map is the sea");
        assert_eq!(net.rivers[0].cells, vec![0, 1, 2]);
    }

    #[test]
    fn land_the_sea_never_reaches_still_drains_to_its_lowest_cell() {
        let heights = [0.9, 0.7, 0.5, 0.6];
        let sites = [[0.5, 0.4], [0.5, 0.5], [0.5, 0.6], [0.5, 0.7]];
        let sub = mesh(&sites, &[(0, 1), (1, 2), (2, 3)], &heights);
        let net = run(&sub, &heights, 0.1);

        assert!(net.lake.iter().all(|l| !l));
        assert_eq!(net.downhill[2], None, "cell 2 is the low point of the fragment");
        assert_eq!(chain(&net, 0), vec![0, 1, 2]);
        assert_eq!(chain(&net, 3), vec![3, 2], "the far side drains back to it too");
        // Both headwaters end in the same dead end, and a tributary inherits that ending.
        assert!(net.rivers.iter().all(|r| r.mouth == Mouth::Endorheic), "{:?}", net.rivers);
        assert_eq!(net.rivers.len(), 2);
    }

    #[test]
    fn no_downhill_chain_revisits_a_cell() {
        let mut heights = plane();
        heights[12] = 0.45;
        let sub = grid(5, 6, &heights);
        let net = run(&sub, &heights, 0.05);
        for c in 0..sub.len() {
            let path = chain(&net, c); // panics on a repeat
            assert!(path.len() <= sub.len());
        }
    }

    #[test]
    fn every_number_that_escapes_is_finite() {
        let mut heights = plane();
        heights[12] = 0.45;
        let sub = grid(5, 6, &heights);
        let net = run(&sub, &heights, 0.05);

        assert!(net.flux.iter().all(|f| f.is_finite() && *f >= 0.0), "{:?}", net.flux);
        for r in &net.rivers {
            assert!(r.flux.iter().all(|f| f.is_finite()));
            assert!(r.points.iter().all(|p| p[0].is_finite() && p[1].is_finite()));
            assert_eq!(r.points.len(), r.flux.len());
            assert!(r.order >= 1);
        }
    }

    #[test]
    fn a_nan_in_the_rainfall_does_not_spread() {
        let heights = plane();
        let sub = grid(5, 6, &heights);
        let mut rain = vec![1.0; sub.len()];
        rain[7] = f32::NAN;
        let net = rivers(&sub, &heights, &rain, SEA_LEVEL, &params(0.05));
        assert!(net.flux.iter().all(|f| f.is_finite()), "{:?}", net.flux);
    }

    #[test]
    fn the_same_input_gives_the_same_network_twice() {
        let mut heights = plane();
        heights[12] = 0.45;
        let sub = grid(5, 6, &heights);
        let a = run(&sub, &heights, 0.05);
        let b = run(&sub, &heights, 0.05);

        assert_eq!(a.flux, b.flux);
        assert_eq!(a.downhill, b.downhill);
        assert_eq!(a.lake, b.lake);
        let course = |n: &RiverNetwork| -> Vec<(Vec<u32>, u32)> {
            n.rivers.iter().map(|r| (r.cells.clone(), r.order)).collect()
        };
        assert_eq!(course(&a), course(&b));
    }

    #[test]
    fn a_long_chain_of_cells_does_not_overflow_the_stack() {
        let n = 20_000;
        let heights: Vec<f32> =
            (0..n).map(|i| 0.9 - 0.5 * i as f32 / n as f32).chain([0.2]).collect();
        let sites: Vec<[f64; 2]> =
            (0..=n).map(|i| [0.5, (i as f64 + 0.5) / (n as f64 + 1.0)]).collect();
        let edges: Vec<(usize, usize)> = (0..n).map(|i| (i, i + 1)).collect();
        let sub = mesh(&sites, &edges, &heights);
        let net = run(&sub, &heights, 0.5);

        assert_eq!(net.downhill[0], Some(1));
        assert!((net.flux[n - 1] - 1.0).abs() < 1e-3, "{}", net.flux[n - 1]);
        assert_eq!(net.rivers.len(), 1);
        assert_eq!(net.rivers[0].mouth, Mouth::Sea);
    }

    #[test]
    fn an_empty_substrate_produces_nothing() {
        let sub = assemble(Vec::new(), Vec::new(), &[]);
        let net = rivers(&sub, &[], &[], SEA_LEVEL, &params(0.006));
        assert!(net.rivers.is_empty() && net.flux.is_empty() && net.downhill.is_empty());
    }
}
