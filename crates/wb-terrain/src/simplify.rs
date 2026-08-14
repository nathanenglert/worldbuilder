//! Stage 4 — simplification, by Visvalingam–Whyatt.
//!
//! Visvalingam rather than Douglas–Peucker because it degrades better: dropping the
//! least-significant *area* keeps a coastline looking like a coastline at low detail,
//! where dropping by perpendicular distance turns bays into chevrons.

use crate::contour::Ring;

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
pub fn simplify_ring(points: &[[f64; 2]], epsilon: f64, aspect: f64) -> Vec<[f64; 2]> {
    let _ = (points, epsilon, aspect);
    todo!("stage 4")
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
    let _ = (rings, detail, aspect);
    todo!("stage 4")
}

/// The detail slider's curve. `1.0` keeps everything; `0.0` is as blunt as it goes.
pub fn epsilon_for(detail: f64) -> f64 {
    // 1e-9 is well under one pixel on any sane raster; 1e-4 is a visible headland.
    let t = 1.0 - detail.clamp(0.0, 1.0);
    1e-9 * (1e-4f64 / 1e-9).powf(t)
}
