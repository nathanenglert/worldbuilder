//! Stage 7 — temperature and rainfall.
//!
//! Latitude sets the temperature; wind carries moisture inland and mountains take it
//! away. The rain shadow is the whole point: it is the one climate effect a reader will
//! notice on a map, because it is why one side of a range is a forest and the other is a
//! desert.

use serde::{Deserialize, Serialize};

use crate::cells::Substrate;
use crate::params::ClimateParams;

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
pub fn climate(sub: &Substrate, heights: &[f32], sea_level: f32, p: &ClimateParams) -> Climate {
    let _ = (sub, heights, sea_level, p);
    todo!("stage 7")
}
