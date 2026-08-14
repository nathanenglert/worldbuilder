use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{}: {message}", path.display())]
    Yaml { path: PathBuf, message: String },

    #[error("{}: no `---` frontmatter block", path.display())]
    MissingFrontmatter { path: PathBuf },

    #[error("duplicate id `{id}`, defined in {} and {}", first.display(), second.display())]
    DuplicateId { id: String, first: PathBuf, second: PathBuf },

    #[error("no world.yaml in {}", root.display())]
    NoWorldFile { root: PathBuf },

    #[error("the map image {} could not be read: {source}", path.display())]
    MapImage {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("the terrain could not be built: {0}")]
    Terrain(String),

    #[error(transparent)]
    Core(#[from] wb_core::Error),
}
