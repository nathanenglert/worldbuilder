use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// The files a refusal is about, named. A refusal that says only *that* something is
/// unsaved leaves the writer hunting for what.
fn unsaved(paths: &[String]) -> String {
    match paths {
        [one] => format!("{one} has changes you have not saved"),
        many => {
            format!("{} files have changes you have not saved: {}", many.len(), many.join(", "))
        }
    }
}

/// Every message here is read by a novelist, not by a programmer, and several of them
/// are refusals. A refusal that does not say what to do instead is just a locked door,
/// so each one names the next move.
#[derive(Debug, Error)]
pub enum Error {
    #[error("nothing is tracking {}", world.display())]
    NotARepository { world: PathBuf },

    /// The finding this whole crate is shaped around: a world folder inside a larger
    /// repository can be *read* at any revision, but branching or switching would move
    /// every other file in that repository too.
    #[error(
        "this world is a folder inside {}, so branching would move that whole \
         repository. History and comparison still work; making a save point or a \
         what-if needs the world folder to be the repository itself.",
        repo.display()
    )]
    NotRepoRoot { repo: PathBuf, world: PathBuf },

    #[error("this repository has no commits yet — make the first save point before branching")]
    Unborn,

    #[error("nothing has changed since the last save point")]
    NothingToSave,

    /// Ordinary after looking at an old revision, and worth naming rather than failing
    /// obscurely two calls later.
    #[error("you are not on a branch right now — switch to one before saving or merging")]
    Detached,

    #[error("{}. Make a save point first, or throw them away.", unsaved(paths))]
    Dirty { paths: Vec<String> },

    /// Inventing an author would put a name the writer never chose into their permanent
    /// history, so this quotes the two commands instead.
    #[error(
        "git does not know who you are yet. Run:\n  \
         git config --global user.name \"Your Name\"\n  \
         git config --global user.email \"you@example.com\""
    )]
    NoAuthor,

    #[error(
        "`{into}` has {behind} commit{} that `{from}` does not, so this is not a clean \
         fast-forward. Merging histories that have both moved is a job for a git client.",
        if *behind == 1 { "" } else { "s" }
    )]
    NotFastForward { from: String, into: String, behind: usize },

    #[error("`{name}` is the branch you are on — switch somewhere else first")]
    CurrentBranch { name: String },

    #[error("there is no branch called `{name}`")]
    NoSuchBranch { name: String },

    #[error("`{name}` already exists")]
    BranchExists { name: String },

    #[error("`{name}` is not a usable branch name")]
    BadBranchName { name: String },

    /// Materializing an old revision copies it out of the object store and onto the disk.
    /// A cap means a world with a folder of raw scans in it fails with a sentence rather
    /// than filling the drive.
    #[error("that revision holds {bytes} bytes, over the {cap}-byte limit for reading one back")]
    TooLarge { bytes: u64, cap: u64 },

    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Git(#[from] git2::Error),
}
