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

    /// The refusal that keeps `create` from being the one operation in the app that can
    /// lose a whole world. A folder holding a `world.yaml` is somebody's world, however
    /// empty the rest of it looks.
    #[error("there is already a world in {}", root.display())]
    WorldExists { root: PathBuf },

    #[error("a world needs a name")]
    UnnamedWorld,

    /// `Scene.source` is the scene's own file, so a `source:` key would parse as unknown,
    /// be dropped, and leave the writer looking at a link the app cannot see. Refusing is
    /// the only outcome that tells them anything.
    #[error("{}: a scene links its prose with `prose:`, not `source:`", path.display())]
    SceneSourceKey { path: PathBuf },

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
