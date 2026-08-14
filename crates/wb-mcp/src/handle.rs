//! The world the server answers from, kept honest against the disk.
//!
//! A long-lived server that loaded once at startup would confidently answer with
//! yesterday's canon — and the writer is editing these files in their own editor while
//! the agent is connected, which is the whole point of files-as-source-of-truth. So
//! every call fingerprints the tree first and reloads when anything moved.
//!
//! The fingerprint is a walk; the reload is a walk plus a parse. Skipping the parse is
//! most of the win, and the walk is what makes the answer correct.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use wb_store::{World, load};

/// Folders whose contents can change what a query answers. `map` is here because a
/// redrawn coastline changes what the ground under a settlement is, and `world.yaml`
/// alone would not notice.
const WATCHED: [&str; 4] = ["entities", "events", "proposals", "map"];

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

/// A cheap summary of every file that could change an answer: path, size, and mtime.
///
/// Deliberately not a content hash — this runs on every call, and the point is to be
/// much cheaper than the parse it avoids.
fn fingerprint(root: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    stamp(&root.join("world.yaml"), &mut hasher);
    for dir in WATCHED {
        walk(&root.join(dir), &mut hasher);
    }
    hasher.finish()
}

fn walk(dir: &Path, hasher: &mut DefaultHasher) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };

    // Sorted, so two runs over an unchanged tree agree regardless of readdir order.
    // Metadata is taken from the directory entry rather than re-`stat`ing the path:
    // this walk runs on every call, and `is_dir()` plus `metadata()` on a `Path` is two
    // more syscalls per file than the entry already has the answers for.
    let mut entries: Vec<std::fs::DirEntry> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let name = entry.file_name();
        if name.to_str().is_some_and(|n| n.starts_with('.')) {
            continue;
        }
        name.hash(hasher);
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => walk(&entry.path(), hasher),
            Ok(_) => {
                if let Ok(meta) = entry.metadata() {
                    stamp_meta(&meta, hasher);
                }
            }
            Err(_) => {}
        }
    }
}

fn stamp(path: &Path, hasher: &mut DefaultHasher) {
    path.hash(hasher);
    if let Ok(meta) = std::fs::metadata(path) {
        stamp_meta(&meta, hasher);
    }
}

fn stamp_meta(meta: &std::fs::Metadata, hasher: &mut DefaultHasher) {
    meta.len().hash(hasher);
    if let Ok(modified) = meta.modified() {
        modified.hash(hasher);
    }
}
