//! The other side of a comparison has to be a world, not a directory of files.
//!
//! `wb-git` extracts a revision into a scratch directory and `wb-propose` compares two
//! worlds; both are tested where they live. What is untested by either is the join —
//! and the join is where a comparison can come out confidently wrong, which is worse
//! than none, because a writer keeps or discards a week of work on the strength of it.

use std::path::{Path, PathBuf};

use wb_propose::diff_worlds;
use wb_store::{World, load};
use worldbuilder_lib::version::borrow_manuscript;

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/vashen")
}

fn vashen() -> World {
    let root = example_root().canonicalize().expect("the example world is there");
    load(&root).unwrap_or_else(|e| panic!("loading {}: {e}", root.display()))
}

/// What `wb_git::materialize` leaves behind: the world's own tracked files, and nothing
/// beside them. Dot-entries are skipped for the same reason git does not carry
/// `.worldbuilder/` — they are derived, not authored.
fn copy_world(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("scratch");
    for entry in std::fs::read_dir(from).expect("readable") {
        let entry = entry.expect("entry");
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        let (source, target) = (entry.path(), to.join(&name));
        if entry.file_type().expect("kind").is_dir() {
            copy_world(&source, &target);
        } else {
            std::fs::copy(&source, &target).expect("copy");
        }
    }
}

/// A revision loaded out of the object store is the same world, and must be reported as
/// the same world — including the half of it that lives outside the repository.
///
/// The first two assertions pin the bug rather than describing it: without the join, an
/// identical revision reads as settling every contradiction the book has with the
/// records. They are also a canary. If the example world's prose ever stops contradicting
/// its records, `resolved` goes empty on its own and this test fails loudly instead of
/// passing for the wrong reason.
#[test]
fn a_materialized_world_still_finds_the_manuscript_that_lives_outside_the_repository() {
    let live = vashen();
    assert!(live.manuscript.is_some(), "the example world has a book; that is the fixture");

    let scratch = std::env::temp_dir().join("wb-materialized-manuscript");
    let _ = std::fs::remove_dir_all(&scratch);
    copy_world(&example_root(), &scratch);
    let mut other = load(&scratch).expect("a copy of a world is a world");

    let stranded = diff_worlds(&live, &other);
    assert!(stranded.is_empty(), "the same records, so not one of them differs");
    assert!(
        !stranded.impact.resolved.is_empty(),
        "and yet: a world that cannot find its manuscript finds no contradiction in it, \
         so an identical revision reads as having settled them all"
    );

    borrow_manuscript(&mut other, &live);

    let honest = diff_worlds(&live, &other);
    assert!(honest.is_empty(), "still the same records");
    assert!(
        honest.impact.resolved.is_empty(),
        "and now it settles nothing, because it changed nothing: {:?}",
        honest.impact.resolved.iter().map(|f| &f.subject).collect::<Vec<_>>()
    );
    assert!(honest.impact.introduced.is_empty(), "and breaks nothing either");
    assert_eq!(honest.impact.before, honest.impact.after, "the same count on both sides");

    let _ = std::fs::remove_dir_all(&scratch);
}
