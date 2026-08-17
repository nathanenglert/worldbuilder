//! What surfaces, what does not, and where the story is leaning on nothing.
//!
//! Roughly 90% of a world never reaches the page, and that is correct — the submerged
//! part is what makes the visible part feel solid. So this report is deliberately not an
//! audit. It never calls anything waste, and the number it leads with is not "how much
//! of your world is unused" but *where the next hour is best spent*.
//!
//! The four quadrants are `skills/iceberg-check`'s, made measurable now that scenes exist:
//!
//! | | Little on the page | Much on the page |
//! |---|---|---|
//! | **Much detail** | Overbuilt — beautiful, and doing its job below the waterline | Load-bearing — the real spine |
//! | **Little detail** | Fine. Most of a world is stubs, and stubs are not debt | **Underbuilt** — the story keeps reaching for something that is not there |
//!
//! Until this crate existed, that skill could only measure how much a world referred to
//! *itself*, and it shipped with a paragraph saying so. This measures the page.

use std::collections::BTreeMap;

use serde::Serialize;
use wb_store::World;

use crate::{Story, mentions};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Quadrant {
    /// The story leans here and there is nothing to lean on. Reported first.
    Underbuilt,
    /// Named often, and built. The spine — worth knowing before changing it.
    LoadBearing,
    /// Rich, and below the waterline. Not a mistake; this is the iceberg working.
    Overbuilt,
    /// Little of either. Most of a world, and perfectly fine.
    Quiet,
}

impl Quadrant {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Underbuilt => "underbuilt",
            Self::LoadBearing => "load-bearing",
            Self::Overbuilt => "overbuilt",
            Self::Quiet => "quiet",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Surfacing {
    pub id: String,
    pub name: String,
    /// Times the prose names it, across every scene.
    pub mentions: usize,
    /// The scenes it appears in, in reading order.
    pub scenes: Vec<String>,
    /// Other records whose facts point here.
    pub referenced_by: usize,
    /// Events naming it as a participant or location.
    pub appears_in: usize,
    /// Scenes listing it as point of view, on the page, or the place it happens.
    ///
    /// Deliberately separate from `mentions`: this is what the *record* claims, and that
    /// is what the *prose* does. A scene that lists somebody in `on_page` who never
    /// appears in the passage is worth being able to see.
    pub cast_in: usize,
    pub facts: usize,
    pub prose_bytes: usize,
    pub quadrant: Quadrant,
    /// One sentence from the book, so the count can be checked rather than trusted.
    pub first_seen: Option<String>,
}

impl Surfacing {
    pub fn surfaced(&self) -> bool {
        self.mentions > 0
    }

    /// How hard the world and the book lean on this record.
    ///
    /// Mentions count double: appearing on the page is what the whole report is about,
    /// and a place named once in one chapter can matter more than one referenced by nine
    /// records that the reader never sees.
    fn pull(&self) -> usize {
        self.mentions * 2 + self.referenced_by + self.appears_in + self.cast_in
    }

    /// How much is actually in the record. A paragraph of prose is worth about as much
    /// as a fact, which is roughly true and stops a long body from swamping everything.
    fn detail(&self) -> usize {
        self.facts + self.prose_bytes / 400
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub entries: Vec<Surfacing>,
    /// Scenes whose prose was read.
    pub scenes_read: usize,
    /// Scene id and why, for the ones that could not be.
    pub unreadable: Vec<(String, String)>,
    pub surfaced: usize,
    pub total: usize,
    pub standing: crate::Standing,
}

impl Report {
    /// Surfaced as a percentage, rounded. `None` when the world is empty rather than 0%,
    /// because 0% of nothing is a statement about the world it does not deserve.
    pub fn ratio(&self) -> Option<u32> {
        (self.total > 0).then(|| (self.surfaced as f64 / self.total as f64 * 100.0).round() as u32)
    }

    pub fn of(&self, quadrant: Quadrant) -> impl Iterator<Item = &Surfacing> {
        self.entries.iter().filter(move |e| e.quadrant == quadrant)
    }
}

/// Measure `world` against `story`.
pub fn report(world: &World, story: &Story) -> Report {
    // A plain loop rather than a fold, because the excerpt has to come from the first
    // mention in *reading* order, and `story.reads` is already in that order.
    let mut tally: BTreeMap<String, (usize, Vec<String>, Option<String>)> = BTreeMap::new();
    for read in &story.reads {
        for mention in &read.mentions {
            let entry = tally.entry(mention.id.clone()).or_insert((0, Vec::new(), None));
            entry.0 += 1;
            if entry.1.last().map(String::as_str) != Some(read.scene.as_str()) {
                entry.1.push(read.scene.clone());
            }
            if entry.2.is_none() {
                entry.2 = Some(mentions::excerpt(read.text(), mention.at, mention.len));
            }
        }
    }

    let mut entries: Vec<Surfacing> = world
        .entities
        .values()
        .map(|entity| {
            let (mentions, scenes, first_seen) =
                tally.get(&entity.id).cloned().unwrap_or((0, Vec::new(), None));

            // `references_to` rather than a hand-rolled scan of fact values. It is the
            // world's own answer to "what points here", and it counts the two kinds a
            // narrower version misses: parentage, and a date anchored to this record.
            // Undercounting them puts a record the world leans on into the wrong column,
            // which is the one mistake this report cannot afford.
            let mut referenced_by = 0;
            let mut appears_in = 0;
            let mut cast_in = 0;
            for reference in world.references_to(&entity.id) {
                if world.scenes.contains_key(&reference.by) {
                    cast_in += 1;
                } else if world.events.contains_key(&reference.by) {
                    appears_in += 1;
                } else {
                    referenced_by += 1;
                }
            }

            Surfacing {
                id: entity.id.clone(),
                name: entity.name.clone(),
                mentions,
                scenes,
                referenced_by,
                appears_in,
                cast_in,
                facts: entity.facts.len(),
                prose_bytes: entity.body.len(),
                quadrant: Quadrant::Quiet,
                first_seen,
            }
        })
        .collect();

    // Thresholds are the world's own medians rather than fixed numbers. "Many facts" in
    // an eleven-record seed world and in a ten-thousand-record one are not the same
    // count, and a constant here would report every young world as uniformly underbuilt.
    //
    // Zeros are excluded from the median, and that is the load-bearing part. Most of a
    // world is stubs nothing points at — that is the iceberg working — but counting them
    // drags both lines to the floor, and "above average" stops distinguishing anything.
    // The comparison that means something is against the records carrying *any* signal.
    let pull_line = median_of_nonzero(entries.iter().map(Surfacing::pull));
    let detail_line = median_of_nonzero(entries.iter().map(Surfacing::detail));

    for e in &mut entries {
        e.quadrant = match (e.pull() >= pull_line, e.detail() >= detail_line) {
            (true, false) => Quadrant::Underbuilt,
            (true, true) => Quadrant::LoadBearing,
            (false, true) => Quadrant::Overbuilt,
            (false, false) => Quadrant::Quiet,
        };
    }

    // Underbuilt first, and within a quadrant the most leaned-on first. That ordering is
    // the report's actual opinion: the top row is where the next hour goes.
    entries.sort_by(|a, b| {
        (a.quadrant as u8)
            .cmp(&(b.quadrant as u8))
            .then_with(|| b.pull().cmp(&a.pull()))
            .then_with(|| a.name.cmp(&b.name))
    });

    let (scenes_read, _) = story.counts();
    let unreadable = story
        .reads
        .iter()
        .filter_map(|r| r.passage.as_ref().err().map(|why| (r.scene.clone(), why.clone())))
        .collect();

    Report {
        surfaced: entries.iter().filter(|e| e.surfaced()).count(),
        total: entries.len(),
        entries,
        scenes_read,
        unreadable,
        standing: story.standing(world),
    }
}

/// The median of the values that are not zero, or `1` when every value is zero — so a
/// world nobody has written about yet reports as uniformly quiet rather than uniformly
/// load-bearing.
fn median_of_nonzero(values: impl Iterator<Item = usize>) -> usize {
    let mut v: Vec<usize> = values.filter(|n| *n > 0).collect();
    if v.is_empty() {
        return 1;
    }
    v.sort_unstable();
    v[v.len() / 2]
}
