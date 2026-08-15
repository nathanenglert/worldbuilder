//! Just enough YAML to change one line of somebody's file and leave the rest alone.
//!
//! Neither half of this is a general YAML implementation, and neither should grow into
//! one. [`scan`] locates bytes; [`emit`] writes the handful of shapes the record model
//! actually contains. Everything else in the format is somebody else's problem, and the
//! writer bails to a canonical rewrite when it meets any of it.

pub mod emit;
pub mod scan;
