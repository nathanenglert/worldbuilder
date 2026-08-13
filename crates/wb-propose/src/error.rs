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

    #[error("`{proposal}` changes `{id}`, which does not exist")]
    UnknownTarget { proposal: String, id: String },

    #[error("`{proposal}` would create `{id}`, which already exists")]
    AlreadyExists { proposal: String, id: String },

    #[error("`{proposal}` removes {attr} = {value} from `{id}`, which has no such fact")]
    NoSuchFact { proposal: String, id: String, attr: String, value: String },

    /// Applying rewrites frontmatter canonically. A key the model does not understand
    /// would not survive that, so the write is refused rather than losing the writer's
    /// data — this tool holds people's life's work.
    #[error(
        "{} carries `{key}`, which this version does not understand; \
         applying would drop it, so nothing was written",
        path.display()
    )]
    WouldDropKey { path: PathBuf, key: String },

    #[error("`{proposal}` has already been {status}")]
    NotPending { proposal: String, status: &'static str },

    #[error(transparent)]
    Store(#[from] wb_store::Error),
}
