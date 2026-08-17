//! **wb-store** — a world on disk, and the queries the map runs against it.
//!
//! Files are the source of truth. A world is a folder the writer owns: Markdown with
//! frontmatter for anything carrying prose, YAML for pure structure. Nothing here
//! requires the application to read, diff, or edit — which is also what makes git
//! branching give "what if" histories for free.
//!
//! ```text
//! my-world/
//! ├── world.yaml            calendar, fuzz defaults, type declarations
//! ├── entities/**/*.md      actors, polities, places, things
//! ├── events/**/*.yaml      dated occurrences that facts anchor to
//! ├── map/                  the imported raster, if there is one
//! └── .worldbuilder/        derived terrain — a build product, never committed
//! ```
//!
//! Events deliberately carry no "effects" block. Facts own their own validity and
//! anchor to events by date, so there is exactly one source of truth, files are
//! order-independent, and re-dating an event replays nothing.

pub mod atomic;
pub mod error;
pub mod freshness;
pub mod frontmatter;
pub mod kin;
pub mod load;
pub mod model;
pub mod paths;
pub mod phrasing;
pub mod sandbox;
pub mod terrain;
pub mod world;
pub mod write;
pub mod yaml;

pub use error::{Error, Result};
pub use load::load;
pub use model::{
    Entity, Event, Fact, ManuscriptSpec, MapSpec, Primitive, Rules, Scene, Span, TypeDef, Value,
    WorldDef,
};
pub use world::{EntityView, FactAt, Hit, Reference, Snapshot, World};
