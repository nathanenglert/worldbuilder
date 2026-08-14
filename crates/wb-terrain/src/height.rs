//! Stage 6 — elevation.
//!
//! Coastal falloff plus hand-placed ranges plus noise. Deliberately not a plate-tectonic
//! simulation: the coastline is already given by the writer's map, and a simulation that
//! disagreed with it would have to be overruled everywhere anyway.

use crate::cells::Substrate;
use crate::mask::Mask;
use crate::params::HeightParams;

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
pub fn heights(sub: &Substrate, mask: &Mask, p: &HeightParams, seed: u64) -> Vec<f32> {
    let _ = (sub, mask, p, seed);
    todo!("stage 6")
}
