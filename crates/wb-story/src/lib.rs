//! **wb-story** — the manuscript, read against the world.
//!
//! Everything else in this workspace reasons about a world that is complete in itself.
//! This crate is the one place that opens a file the world does not own, and it exists
//! because the question a novelist actually has is not "is my world consistent" but
//! *"which of this is on the page, and does the page agree with it?"*
//!
//! Three layers, each usable without the ones above it:
//!
//! - [`manuscript`] resolves a scene's `prose:` link and returns the passage. Read-only
//!   by construction — there is no function here that writes.
//! - [`mentions`] finds the world inside that prose, conservatively and auditably.
//! - [`iceberg`] turns the two into the report DESIGN.md §8 calls the thing no other
//!   tool can show a writer: what surfaces, what does not, and where the story is
//!   leaning on something that was never built.
//!
//! [`canon`] sits alongside them and produces `wb_check::Finding`s, so a contradiction
//! found in prose renders in the same panel, with the same vocabulary, as one found in
//! the records. `wb-check` itself never learns to read the disk.

pub mod canon;
pub mod iceberg;
pub mod manuscript;
pub mod mentions;

use std::collections::BTreeMap;

use serde::Serialize;
use wb_store::World;

pub use manuscript::Passage;
pub use mentions::{Mention, Via};

/// One scene, with whatever its link could be made to yield.
#[derive(Debug, Clone)]
pub struct Read {
    pub scene: String,
    /// `Err` carries the reason, phrased for the writer. A scene whose prose cannot be
    /// read is still a scene — the link may simply not be written yet — so this is never
    /// an error that stops anything.
    pub passage: Result<Passage, String>,
    pub mentions: Vec<Mention>,
}

impl Read {
    pub fn text(&self) -> &str {
        self.passage.as_ref().map(|p| p.text.as_str()).unwrap_or("")
    }
}

/// The whole manuscript as this world sees it: every scene, read and scanned once.
///
/// Built in one pass and then queried, because the alternative — resolving a link per
/// question — reads the same chapter once per scene in it, and a chapter usually holds
/// several scenes.
#[derive(Debug, Clone)]
pub struct Story {
    pub reads: Vec<Read>,
    /// Reading order: scene id → position, derived from the manuscript itself.
    pub order: BTreeMap<String, usize>,
    /// True when the world declares a manuscript at all.
    pub linked: bool,
}

/// Why a story might have nothing in it, in words a panel can show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    /// No `manuscript:` in `world.yaml`. Not a problem — most worlds are like this.
    Unlinked,
    /// Declared, but the folder is not there.
    RootMissing,
    /// Linked and read.
    Linked,
}

impl Story {
    /// Read every scene in `world` and scan each passage for mentions.
    pub fn read(world: &World) -> Self {
        let Some(base) = manuscript::root(world) else {
            return Self { reads: Vec::new(), order: BTreeMap::new(), linked: false };
        };

        let declared = world.manuscript.as_ref().map(|m| m.order.clone()).unwrap_or_default();
        let chapters = manuscript::chapters(&base, &declared);
        let index = mentions::index(world);

        let mut reads: Vec<Read> = world
            .scenes
            .values()
            .map(|scene| {
                let passage = match &scene.prose {
                    None => Err("this scene is not linked to any prose yet".to_string()),
                    Some(link) => manuscript::read(&base, link),
                };
                let mentions =
                    passage.as_ref().map(|p| mentions::scan(&index, &p.text)).unwrap_or_default();
                Read { scene: scene.id.clone(), passage, mentions }
            })
            .collect();

        // Reading order is *derived*, never stored: the position of the scene's chapter
        // in the manuscript, then the position of its anchor within that chapter. The
        // book is the order, so reordering chapters reorders the story path with no
        // bookkeeping to get stale — and a scene whose prose is missing sorts last
        // rather than claiming a position it cannot justify.
        let rank = |r: &Read| -> (usize, usize) {
            let Ok(p) = &r.passage else { return (usize::MAX, usize::MAX) };
            let chapter = chapters.iter().position(|c| *c == p.file).unwrap_or(usize::MAX - 1);
            let within = p
                .anchor
                .as_ref()
                .and_then(|_| std::fs::read_to_string(base.join(&p.file)).ok())
                .and_then(|whole| {
                    whole
                        .find(p.text.trim_end())
                        .or_else(|| p.heading.as_ref().and_then(|h| whole.find(h.as_str())))
                })
                .unwrap_or(0);
            (chapter, within)
        };

        reads.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.scene.cmp(&b.scene)));
        let order = reads.iter().enumerate().map(|(i, r)| (r.scene.clone(), i)).collect();

        Self { reads, order, linked: true }
    }

    pub fn standing(&self, world: &World) -> Standing {
        if !self.linked {
            return Standing::Unlinked;
        }
        match manuscript::root(world) {
            Some(base) if base.is_dir() => Standing::Linked,
            _ => Standing::RootMissing,
        }
    }

    pub fn get(&self, scene: &str) -> Option<&Read> {
        self.reads.iter().find(|r| r.scene == scene)
    }

    /// Scenes whose prose was read, and scenes whose link went nowhere.
    pub fn counts(&self) -> (usize, usize) {
        let read = self.reads.iter().filter(|r| r.passage.is_ok()).count();
        (read, self.reads.len() - read)
    }
}
