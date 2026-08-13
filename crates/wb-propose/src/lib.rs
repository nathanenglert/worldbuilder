//! **wb-propose** — the review queue between a suggestion and canon.
//!
//! Nothing an agent writes reaches the world directly. Changes land here as proposals,
//! a human accepts or rejects them, and only then do files change. That costs nothing
//! extra, because a novelist wants canon-versus-speculative staging anyway — one
//! mechanism serves both, and an agent having a bad session cannot damage anything.
//!
//! The queue earns its keep through [`impact`]: before accepting, the writer sees which
//! contradictions a proposal settles and which it creates. The check engine already
//! answers both questions, so the only work here is the diff.
//!
//! ```no_run
//! let world = wb_store::load("examples/vashen").unwrap();
//! let mut proposals = wb_propose::store::load_all("examples/vashen").unwrap();
//!
//! for proposal in proposals.iter_mut().filter(|p| p.is_pending()) {
//!     let impact = wb_propose::impact(&world, proposal).unwrap();
//!     if impact.breaks_something() {
//!         wb_propose::reject(proposal).unwrap();
//!     }
//! }
//! ```
//!
//! **One limitation, stated plainly.** Applying rewrites a record's frontmatter
//! canonically, so comments inside frontmatter do not survive. Prose bodies are
//! preserved verbatim, a key the model does not understand is refused rather than
//! dropped, and every write is an ordinary git diff the writer can inspect or revert.

pub mod apply;
pub mod error;
pub mod impact;
pub mod model;
pub mod store;

pub use apply::{FileEdit, preview, simulate};
pub use error::{Error, Result};
pub use impact::{Impact, impact};
pub use model::{Change, Proposal, Status};

use std::path::PathBuf;

use wb_store::World;

/// Write a proposal's files, then record the decision. Returns what changed on disk.
///
/// Rendering happens in full before anything is written, so a proposal that cannot be
/// applied completely is not applied in part.
pub fn accept(world: &World, proposal: &mut Proposal) -> Result<Vec<PathBuf>> {
    let written = apply::accept(world, proposal)?;
    store::set_status(proposal, Status::Accepted)?;
    Ok(written)
}

/// Decline a proposal. The file stays, carrying the decision.
pub fn reject(proposal: &mut Proposal) -> Result<PathBuf> {
    store::set_status(proposal, Status::Rejected)
}
