//! The world the server answers from, kept honest against the disk.
//!
//! A long-lived server that loaded once at startup would confidently answer with
//! yesterday's canon — and the writer is editing these files in their own editor while
//! the agent is connected, which is the whole point of files-as-source-of-truth. So
//! every call fingerprints the tree first and reloads when anything moved.
//!
//! The fingerprint is a walk; the reload is a walk plus a parse. Skipping the parse is
//! most of the win, and the walk is what makes the answer correct.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use wb_store::freshness::fingerprint;
use wb_store::{World, load};

#[derive(Debug)]
pub struct WorldHandle {
    root: PathBuf,
    state: Mutex<State>,
}

#[derive(Debug)]
struct State {
    world: World,
    fingerprint: u64,
    /// How many times the tree changed under us. Reported by `describe_world` so a
    /// stale answer is diagnosable rather than mysterious.
    reloads: u64,
}

impl WorldHandle {
    pub fn open(root: impl AsRef<Path>) -> wb_store::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let world = load(&root)?;
        let fingerprint = fingerprint(&root);
        Ok(Self { root, state: Mutex::new(State { world, fingerprint, reloads: 0 }) })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Run a query against the current world, reloading first if the files moved.
    ///
    /// Reload failure is not fatal: a half-saved file in the writer's editor should not
    /// take the server down, so the last good world keeps answering and the error is
    /// surfaced on the next call that can report it.
    pub fn with<T>(&self, f: impl FnOnce(&World) -> T) -> Result<T, String> {
        let mut state = self.state.lock().map_err(|_| "world state is poisoned".to_string())?;

        let current = fingerprint(&self.root);
        if current != state.fingerprint {
            match load(&self.root) {
                Ok(world) => {
                    state.world = world;
                    state.fingerprint = current;
                    state.reloads += 1;
                }
                Err(e) => {
                    return Err(format!(
                        "the world on disk changed and no longer loads: {e}\n\
                         Answering was refused rather than answering from a stale copy. \
                         Fix the file and try again."
                    ));
                }
            }
        }

        Ok(f(&state.world))
    }

    pub fn reloads(&self) -> u64 {
        self.state.lock().map(|s| s.reloads).unwrap_or(0)
    }
}
