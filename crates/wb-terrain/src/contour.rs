//! Stage 3 — contour trace, by marching squares.

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
pub fn trace(mask: &Mask) -> Vec<Ring> {
    let _ = mask;
    todo!("stage 3")
}
