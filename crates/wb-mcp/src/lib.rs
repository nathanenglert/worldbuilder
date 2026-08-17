//! **wb-mcp** — the world, exposed to whatever agent the writer already uses.
//!
//! The design claim this crate makes structurally true: *AI is not the main driver.*
//! The app is fully functional with nothing attached. This server is an optional
//! **client** of the data model, never a layer inside it — so there are no API keys in
//! the app, no vendor lock-in, and no inference cost on anyone shipping it.
//!
//! Two properties matter more than the tool list:
//!
//! **It cannot write to the world.** Every write tool files a proposal, and a human
//! accepts it in the app. There is no accept tool and no way to add one from the agent
//! side. A novelist loses trust in a tool like this in exactly one bad session, and the
//! proposal queue costs nothing because writers want canon-versus-speculative staging
//! anyway.
//!
//! **It never resolves the writer's uncertainty.** A fact that is `maybe` true at a date
//! is reported as `maybe`. A `possible` consistency finding is reported as possible,
//! because that is the shape a deliberate mystery takes. Detection is deterministic and
//! offline; judgement is where an agent starts being useful.
//!
//! ```no_run
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! use rmcp::ServiceExt;
//!
//! let server = wb_mcp::WorldServer::open("examples/vashen")?;
//! server.serve(rmcp::transport::stdio()).await?.waiting().await?;
//! # Ok(())
//! # }
//! ```

pub mod change;
pub mod dto;
pub mod handle;
pub mod notes;
pub mod overview;
pub mod server;
pub mod story;
pub mod terrain;

pub use change::{ChangeInput, FactInput, ValueInput};
pub use handle::WorldHandle;
pub use overview::WorldOverview;
pub use server::{INSTRUCTIONS, WorldServer};
