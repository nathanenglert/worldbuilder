//! Stage 7 — temperature and rainfall.
//!
//! Latitude sets the temperature; wind carries moisture inland and mountains take it
//! away. The rain shadow is the whole point: it is the one climate effect a reader will
//! notice on a map, because it is why one side of a range is a forest and the other is a
//! desert.

use serde::{Deserialize, Serialize};

use crate::cells::Substrate;
use crate::params::ClimateParams;

/// What air arriving off the open sea carries, and the ceiling a long fetch may build to.
/// Without the ceiling an ocean-crossing wind arrives at the coast unboundedly wet and the
/// normalization in step 6 flattens every continent to nothing.
const OPEN_SEA_MOISTURE: f64 = 1.0;
const MAX_MOISTURE: f64 = 1.5;

/// Sea at or below `COLD_SEA` evaporates nothing; at or above `WARM_SEA`, everything the
/// parameter allows. Degrees Celsius.
const COLD_SEA: f64 = -2.0;
const WARM_SEA: f64 = 25.0;

/// How square to the wind an edge may be before it carries nothing. A due-west wind is
/// `sin(270°)`, which is `-1` to within `2e-16` rather than exactly, so every north-south
/// edge on the map comes out a hair downwind. Counting those would let a windward-edge
/// cell inherit from a crosswind neighbour instead of from the sea, and which neighbour
/// would depend on the last bit of a sine.
const CROSSWIND: f64 = 1e-9;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Climate {
    /// Degrees Celsius.
    pub temperature: Vec<f32>,
    /// Normalized `0.0..=1.0`, where `1.0` is the wettest cell on the map. Relative
    /// rather than absolute: the parameters are a writer's dials, not a rain gauge, and
    /// pretending they yield millimetres would be a false precision.
    pub precipitation: Vec<f32>,
}

/// Temperature and rainfall per cell.
///
/// **Temperature** is latitude then altitude:
/// `lerp(equator_temp, pole_temp, |y - equator| / pole_distance)`, clamped, minus
/// `lapse * max(0, height - sea_level)`.
///
/// **Rainfall** is a single upwind-to-downwind sweep, which is valid because the wind
/// direction gives a total order:
///
/// 1. Turn `wind_bearing` — the compass direction the wind blows *from* — into a unit
///    vector in map space. Remember `y` grows southward: a 270° westerly blows toward
///    increasing `x`.
/// 2. Sort cells by their projection onto that vector, upwind first, correcting `x` by
///    `aspect`. Sweep in that order so every cell is visited after its upwind neighbours.
/// 3. Each cell inherits moisture from its upwind neighbours, weighted by how closely the
///    edge to them aligns with the wind. Cells with no upwind neighbour — the windward
///    edge of the map — start from open-sea moisture.
/// 4. Over sea, moisture *gains* `evaporation`, scaled by how warm the water is. Cold
///    seas are poor evaporators, which is why high latitudes are dry.
/// 5. Over land, rain falls as `base_rain` of the moisture carried, plus
///    `orographic * max(0, height - upwind height)` — the lift term. Subtract what falls.
/// 6. Normalize the result so the wettest cell is `1.0`.
///
/// A cell can only rain out what it carries, so moisture never goes negative and a long
/// continental interior dries out on its own.
///
/// Rain over sea is computed and recorded like any other cell. It is not interesting in
/// itself, but leaving ocean at zero would put a false step at every shoreline and would
/// skew what the normalization divides by.
pub fn climate(sub: &Substrate, heights: &[f32], sea_level: f32, p: &ClimateParams) -> Climate {
    let n = sub.len();
    let temperature = temperatures(sub, heights, sea_level, p);
    let wind = wind_vector(p.wind_bearing);

    // Upwind first. A cell's inflow is only defined once every cell upwind of it has been
    // swept, and projection onto the wind is exactly that order: an edge that points
    // downwind strictly increases the projection. Ties break by index, never by chance.
    let along: Vec<f64> =
        sub.sites.iter().map(|s| s[0] * sub.aspect * wind[0] + s[1] * wind[1]).collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| along[a].total_cmp(&along[b]).then(a.cmp(&b)));

    // What each cell hands on to the cells downwind of it, after evaporation and rain.
    let mut carried = vec![0.0f64; n];
    let mut rain = vec![0.0f64; n];

    for &c in &order {
        let (mut moisture, upwind_height) = inflow(sub, heights, &carried, wind, c);

        if !sub.is_land[c] {
            let warmth =
                ((f64::from(temperature[c]) - COLD_SEA) / (WARM_SEA - COLD_SEA)).clamp(0.0, 1.0);
            moisture = (moisture + f64::from(p.evaporation) * warmth).min(MAX_MOISTURE);
        }

        let lift = f64::from(p.orographic) * (f64::from(heights[c]) - upwind_height).max(0.0);
        // A cell can only rain out what it carries; min/max rather than clamp because a
        // writer may set base_rain negative and clamp panics when its bounds cross.
        let fall = (moisture * (f64::from(p.base_rain) + lift)).min(moisture).max(0.0);
        carried[c] = moisture - fall;
        rain[c] = fall;
    }

    let wettest = rain.iter().copied().fold(0.0, f64::max);
    let precipitation = if wettest > 0.0 {
        rain.iter().map(|r| finite((r / wettest) as f32).clamp(0.0, 1.0)).collect()
    } else {
        // A map where nothing rains is dry, not undefined.
        vec![0.0; n]
    };

    Climate { temperature, precipitation }
}

/// Latitude then altitude, per cell.
fn temperatures(sub: &Substrate, heights: &[f32], sea_level: f32, p: &ClimateParams) -> Vec<f32> {
    // A pole distance of zero is the limit of the formula, not a division by zero: the
    // gradient becomes infinite and everything off the equator sits at the pole.
    let span = p.pole_distance.abs().max(f64::MIN_POSITIVE);
    let (warm, cold) = (f64::from(p.equator_temp), f64::from(p.pole_temp));

    sub.sites
        .iter()
        .zip(heights)
        .map(|(s, h)| {
            let t = ((s[1] - p.equator).abs() / span).clamp(0.0, 1.0);
            let above_sea = f64::from((h - sea_level).max(0.0));
            finite((warm + t * (cold - warm) - f64::from(p.lapse) * above_sea) as f32)
        })
        .collect()
}

/// Where the wind blows *toward*, as a unit vector in map space.
///
/// `bearing` is the compass direction it comes *from*, so the vector is the reverse of it,
/// and `y` grows southward: a 270° westerly gives `(1, 0)`, blowing toward increasing `x`.
fn wind_vector(bearing: f64) -> [f64; 2] {
    let r = bearing.to_radians();
    [-r.sin(), r.cos()]
}

/// Moisture arriving at `c`, and the height it arrives over.
///
/// A neighbour is upwind when the edge from it to `c` points downwind; how squarely it
/// does so is its share. Sites are aspect-corrected first, or the wind bends on a
/// non-square map.
fn inflow(
    sub: &Substrate,
    heights: &[f32],
    carried: &[f64],
    wind: [f64; 2],
    c: usize,
) -> (f64, f64) {
    let (mut weight, mut moisture, mut height) = (0.0, 0.0, 0.0);

    for &nb in &sub.neighbors[c] {
        let nb = nb as usize;
        let dx = (sub.sites[c][0] - sub.sites[nb][0]) * sub.aspect;
        let dy = sub.sites[c][1] - sub.sites[nb][1];
        let len = f64::hypot(dx, dy);
        if len <= 0.0 {
            continue; // coincident sites have no direction to contribute
        }
        let w = (dx * wind[0] + dy * wind[1]) / len;
        if w > CROSSWIND {
            weight += w;
            moisture += w * carried[nb];
            height += w * f64::from(heights[nb]);
        }
    }

    if weight > 0.0 {
        (moisture / weight, height / weight)
    } else {
        // The windward edge of the map: the air arrives off open sea, having climbed
        // nothing on the way in.
        (OPEN_SEA_MOISTURE, f64::from(heights[c]))
    }
}

/// Terrain is a cache key and feeds a biome table, so a nonsense parameter must degrade to
/// a finite world rather than seed NaN through everything downstream. Applied after the
/// narrowing cast, which turns a merely enormous `f64` into an infinity.
fn finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEA_LEVEL: f32 = 0.32;

    /// A `cols × rows` lattice of sites with 4-connected adjacency: the same shape of graph
    /// stage 5 produces, with the geometry known by hand.
    fn grid(cols: usize, rows: usize) -> Substrate {
        let mut sites = Vec::new();
        let mut neighbors = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                sites.push([(i as f64 + 0.5) / cols as f64, (j as f64 + 0.5) / rows as f64]);
                let mut nb = Vec::new();
                if i > 0 {
                    nb.push((j * cols + i - 1) as u32);
                }
                if i + 1 < cols {
                    nb.push((j * cols + i + 1) as u32);
                }
                if j > 0 {
                    nb.push(((j - 1) * cols + i) as u32);
                }
                if j + 1 < rows {
                    nb.push(((j + 1) * cols + i) as u32);
                }
                neighbors.push(nb);
            }
        }
        let n = sites.len();
        Substrate {
            aspect: 1.0,
            polygons: vec![Vec::new(); n],
            is_land: vec![true; n],
            sites,
            neighbors,
        }
    }

    fn westerly() -> ClimateParams {
        ClimateParams { wind_bearing: 270.0, ..Default::default() }
    }

    #[test]
    fn a_westerly_blows_toward_increasing_x() {
        let w = wind_vector(270.0);
        assert!((w[0] - 1.0).abs() < 1e-12 && w[1].abs() < 1e-12, "270° should blow east: {w:?}");

        // y grows southward, so wind from the north blows toward increasing y.
        let n = wind_vector(0.0);
        assert!((n[1] - 1.0).abs() < 1e-12 && n[0].abs() < 1e-12, "0° should blow south: {n:?}");

        let e = wind_vector(90.0);
        assert!((e[0] + 1.0).abs() < 1e-12, "90° should blow west: {e:?}");

        let s = wind_vector(180.0);
        assert!((s[1] + 1.0).abs() < 1e-12, "180° should blow north: {s:?}");
    }

    #[test]
    fn temperature_falls_away_from_the_equator() {
        let sub = grid(1, 5); // one column, five latitudes at y = 0.1 .. 0.9
        let h = vec![SEA_LEVEL; 5];
        let p = ClimateParams {
            equator: 0.0,
            pole_distance: 1.0,
            equator_temp: 30.0,
            pole_temp: -20.0,
            lapse: 0.0,
            ..Default::default()
        };
        let c = climate(&sub, &h, SEA_LEVEL, &p);

        for w in c.temperature.windows(2) {
            assert!(w[1] < w[0], "temperature should fall southward here: {:?}", c.temperature);
        }
        // 30 - 50y at y = 0.1 and y = 0.9.
        assert!((c.temperature[0] - 25.0).abs() < 1e-4, "{:?}", c.temperature);
        assert!((c.temperature[4] + 15.0).abs() < 1e-4, "{:?}", c.temperature);
    }

    #[test]
    fn temperature_falls_with_altitude() {
        let sub = grid(3, 1); // one latitude, so only height can separate them
        let h = vec![SEA_LEVEL, SEA_LEVEL + 0.3, SEA_LEVEL + 0.6];
        let p = ClimateParams { lapse: 10.0, ..Default::default() };
        let c = climate(&sub, &h, SEA_LEVEL, &p);

        assert!((c.temperature[0] - c.temperature[1] - 3.0).abs() < 1e-3, "{:?}", c.temperature);
        assert!((c.temperature[0] - c.temperature[2] - 6.0).abs() < 1e-3, "{:?}", c.temperature);
    }

    #[test]
    fn a_westerly_wets_the_western_edge_more_than_the_eastern_interior() {
        let (cols, rows) = (8, 3);
        let sub = grid(cols, rows);
        let h = vec![SEA_LEVEL + 0.1; cols * rows];
        let c = climate(&sub, &h, SEA_LEVEL, &westerly());

        for j in 0..rows {
            let row: Vec<f32> = (0..cols).map(|i| c.precipitation[j * cols + i]).collect();
            assert!(row[0] > row[cols - 1], "the interior should be drier: {row:?}");
            for w in row.windows(2) {
                assert!(w[1] < w[0], "flat land should dry out monotonically inland: {row:?}");
            }
        }
    }

    #[test]
    fn an_edge_square_to_the_wind_carries_nothing() {
        // Under a due-west wind the rows of a lattice are independent: nothing crosses a
        // north-south edge. They come out identical, or a rounding-error crosswind is
        // leaking moisture between latitudes.
        let (cols, rows) = (6, 4);
        let sub = grid(cols, rows);
        let h = vec![SEA_LEVEL + 0.1; cols * rows];
        let c = climate(&sub, &h, SEA_LEVEL, &westerly());

        for j in 1..rows {
            for i in 0..cols {
                let (first, this) = (c.precipitation[i], c.precipitation[j * cols + i]);
                assert_eq!(first, this, "row {j} column {i} differs from row 0");
            }
        }
    }

    #[test]
    fn the_horizontal_squash_is_undone_before_the_wind_sees_it() {
        // Two cells joined by one diagonal edge, under a wind blowing east and a little
        // north. Stretched wide, the edge lies along the wind and `b` is downwind of `a`;
        // square, it lies across the wind and `a` is downwind of `b`. The downwind cell is
        // the drier one, so which is drier is the whole of the aspect correction.
        let pair = |aspect: f64| Substrate {
            aspect,
            sites: vec![[0.0, 0.5], [0.4, 0.9]],
            polygons: vec![Vec::new(); 2],
            neighbors: vec![vec![1], vec![0]],
            is_land: vec![true; 2],
        };
        let h = vec![SEA_LEVEL + 0.1; 2];
        let p = ClimateParams { wind_bearing: 216.87, ..Default::default() };

        let square = climate(&pair(1.0), &h, SEA_LEVEL, &p);
        assert!(square.precipitation[0] < square.precipitation[1], "{:?}", square.precipitation);

        let wide = climate(&pair(3.0), &h, SEA_LEVEL, &p);
        assert!(wide.precipitation[1] < wide.precipitation[0], "{:?}", wide.precipitation);
    }

    #[test]
    fn a_ridge_leaves_the_downwind_side_dry_and_reversing_the_wind_swaps_which_side() {
        let (cols, rows) = (9, 3);
        let sub = grid(cols, rows);
        let mut h = vec![SEA_LEVEL + 0.1; cols * rows];
        for j in 0..rows {
            h[j * cols + 4] = 0.9; // a ridge straight down the middle column
        }

        let blowing_east = climate(&sub, &h, SEA_LEVEL, &westerly());
        let blowing_west =
            climate(&sub, &h, SEA_LEVEL, &ClimateParams { wind_bearing: 90.0, ..westerly() });

        let side = |c: &Climate, from: usize, to: usize| -> f32 {
            let cells: Vec<f32> = (from..to)
                .flat_map(|i| (0..rows).map(move |j| j * cols + i))
                .map(|k| c.precipitation[k])
                .collect();
            cells.iter().sum::<f32>() / cells.len() as f32
        };

        assert!(
            side(&blowing_east, 5, 9) < side(&blowing_east, 0, 4),
            "a westerly should leave the east dry"
        );
        assert!(
            side(&blowing_west, 0, 4) < side(&blowing_west, 5, 9),
            "reversing the bearing should move the shadow to the west"
        );

        // The step across the ridge is the shadow itself, not the slow inland drying:
        // column 5 sits one cell downwind of the crest, column 3 one cell upwind.
        for j in 0..rows {
            let (upwind, downwind) = (
                blowing_east.precipitation[j * cols + 3],
                blowing_east.precipitation[j * cols + 5],
            );
            assert!(downwind < upwind * 0.5, "shadow too weak: {downwind} vs {upwind}");
        }

        // The two runs are mirror images of one another, so their maps must be too.
        for j in 0..rows {
            for i in 0..cols {
                let a = blowing_east.precipitation[j * cols + i];
                let b = blowing_west.precipitation[j * cols + (cols - 1 - i)];
                assert!((a - b).abs() < 1e-6, "({i}, {j}) is not the mirror of its twin: {a} {b}");
            }
        }
    }

    #[test]
    fn a_warm_sea_upwind_wets_the_coast_more_than_a_cold_one() {
        let (cols, rows) = (6, 2);
        let mut sub = grid(cols, rows);
        let mut h = vec![SEA_LEVEL + 0.05; cols * rows];
        for j in 0..rows {
            for i in 0..3 {
                sub.is_land[j * cols + i] = false;
                h[j * cols + i] = 0.1;
            }
        }
        // Row 0 sits on the equator at 30°C, row 1 a full pole distance away at -20°C.
        let p = ClimateParams {
            equator: 0.25,
            pole_distance: 0.5,
            equator_temp: 30.0,
            pole_temp: -20.0,
            ..westerly()
        };
        let c = climate(&sub, &h, SEA_LEVEL, &p);

        for i in 3..cols {
            let (warm, cold) = (c.precipitation[i], c.precipitation[cols + i]);
            assert!(warm > cold, "column {i}: cold seas should feed less rain ({warm} vs {cold})");
        }
    }

    #[test]
    fn precipitation_is_normalized_with_a_wettest_cell_at_one() {
        let (cols, rows) = (7, 4);
        let sub = grid(cols, rows);
        let mut h = vec![SEA_LEVEL + 0.1; cols * rows];
        h[2 * cols + 3] = 0.95;
        let c = climate(&sub, &h, SEA_LEVEL, &westerly());

        assert!(c.precipitation.iter().all(|v| (0.0..=1.0).contains(v)), "{:?}", c.precipitation);
        assert!(
            c.precipitation.contains(&1.0),
            "something must be the wettest cell: {:?}",
            c.precipitation
        );
    }

    #[test]
    fn every_value_is_finite_even_for_degenerate_parameters() {
        let (cols, rows) = (5, 5);
        let sub = grid(cols, rows);
        let h = vec![SEA_LEVEL + 0.2; cols * rows];
        let degenerate = ClimateParams {
            pole_distance: 0.0, // the limit case, not a division by zero
            wind_bearing: f64::NAN,
            equator: f64::INFINITY,
            ..Default::default()
        };
        // Arithmetic that stays finite in f64 and overflows on the way down to f32.
        let enormous = ClimateParams {
            pole_distance: 0.0,
            pole_temp: f32::MAX,
            lapse: -f32::MAX,
            ..Default::default()
        };

        for p in [ClimateParams::default(), degenerate, enormous] {
            let c = climate(&sub, &h, SEA_LEVEL, &p);
            assert!(c.temperature.iter().all(|v| v.is_finite()), "{:?}", c.temperature);
            assert!(c.precipitation.iter().all(|v| v.is_finite()), "{:?}", c.precipitation);
        }
    }

    #[test]
    fn a_map_where_nothing_rains_is_dry_rather_than_undefined() {
        let sub = grid(4, 4);
        let h = vec![SEA_LEVEL + 0.1; 16];
        let p = ClimateParams { base_rain: 0.0, orographic: 0.0, ..westerly() };
        let c = climate(&sub, &h, SEA_LEVEL, &p);

        assert_eq!(c.precipitation, vec![0.0; 16]);
    }

    #[test]
    fn the_same_substrate_gives_the_same_climate_every_time() {
        let (cols, rows) = (12, 6);
        let sub = grid(cols, rows);
        let h: Vec<f32> =
            (0..cols * rows).map(|i| SEA_LEVEL + 0.01 * ((i * 7) % 13) as f32).collect();
        let p = ClimateParams { wind_bearing: 213.0, ..Default::default() };

        let a = climate(&sub, &h, SEA_LEVEL, &p);
        let b = climate(&sub, &h, SEA_LEVEL, &p);
        assert_eq!(a.temperature, b.temperature);
        assert_eq!(a.precipitation, b.precipitation);
    }
}
