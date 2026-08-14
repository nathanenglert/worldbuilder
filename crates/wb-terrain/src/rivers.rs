//! Stage 8 — drainage.
//!
//! Priority-flood to make every basin drain, steepest descent to pick a course, flow
//! accumulation to decide which courses are big enough to be rivers.

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
pub fn rivers(
    sub: &Substrate,
    heights: &[f32],
    precipitation: &[f32],
    sea_level: f32,
    p: &RiverParams,
) -> RiverNetwork {
    let _ = (sub, heights, precipitation, sea_level, p);
    todo!("stage 8")
}
