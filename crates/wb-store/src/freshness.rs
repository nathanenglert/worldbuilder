//! Noticing that the world moved under you.
//!
//! Files are the source of truth, which means the writer is editing them in their own
//! editor while this process holds a loaded copy. Anything that answers questions from
//! that copy has to check first, or it confidently answers with yesterday's canon.
//!
//! Two granularities, for two different jobs:
//!
//! - [`fingerprint`] stamps the whole tree, and answers "should I reload?". It is a walk
//!   without a parse, which is most of the saving over reloading unconditionally.
//! - [`revision`] hashes one file's bytes, and answers "is this still the file I read?".
//!   It is what a save checks before overwriting an edit someone else made in between.
//!
//! `revision` deliberately hashes content rather than metadata. Modification time has
//! one-second granularity on some filesystems, which is exactly the wrong resolution for
//! "another editor saved between the two clicks that opened and committed this form".

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Folders whose contents can change what a query answers. `map` is here because a
/// redrawn coastline changes what the ground under a settlement is, and `world.yaml`
/// alone would not notice.
///
/// The *manuscript* is deliberately absent: it lives outside the world root by design,
/// and a novel being edited in Scrivener would otherwise reload the world on every
/// keystroke. Prose is fingerprinted separately, where it is read.
const WATCHED: [&str; 5] = ["entities", "events", "scenes", "proposals", "map"];

/// A stamp over the whole world tree: paths, sizes, and modification times.
///
/// Equal stamps mean nothing observable moved. Unequal stamps mean reload — including
/// the case where a file was touched without changing, which costs a reload nobody
/// needed and is the cheap side of the trade.
pub fn fingerprint(root: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    stamp(&root.join("world.yaml"), &mut hasher);
    for dir in WATCHED {
        walk(&root.join(dir), &mut hasher);
    }
    hasher.finish()
}

/// A content hash of one file, as a short hex string.
///
/// Hand-rolled FNV-1a rather than `DefaultHasher`, whose algorithm is explicitly allowed
/// to change between releases — a revision that means something different after a
/// toolchain bump would silently stop protecting anything.
pub fn revision(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The revision of whatever is at `path` now, or `None` if there is nothing there.
pub fn revision_of(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|bytes| revision(&bytes))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_bytes_always_give_the_same_revision() {
        assert_eq!(revision(b"id: place_marrow\n"), revision(b"id: place_marrow\n"));
        assert_ne!(revision(b"population: 9000"), revision(b"population: 9001"));
        assert_eq!(revision(b"").len(), 16, "a fixed-width hex stamp, even for nothing");
    }

    /// A whitespace-only difference is still a difference: the writer's file changed,
    /// and overwriting it would still discard what they did.
    #[test]
    fn whitespace_counts_as_a_change() {
        assert_ne!(revision(b"a: 1\n"), revision(b"a: 1\r\n"));
        assert_ne!(revision(b"a: 1"), revision(b"a: 1\n"));
    }

    #[test]
    fn a_missing_folder_does_not_break_the_walk() {
        let stamp = fingerprint(Path::new("/no/such/world"));
        assert_eq!(stamp, fingerprint(Path::new("/no/such/world")), "and it is stable");
    }
}
