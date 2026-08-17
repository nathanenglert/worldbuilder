//! Checking the prose against the world at the date it happens.
//!
//! DESIGN.md §5 lists this rule and then defers it — *"**Scene contradictions** wait for
//! Slice 5, when scenes exist."* It lives here rather than in `wb-check` for one reason:
//! it needs to read a file the world does not own, and `wb-check`'s whole claim is that
//! it is interval arithmetic over facts the writer already stated — instant, offline, and
//! incapable of inventing a contradiction that is not there. Teaching it to open the
//! manuscript would cost that. So `wb-check` owns the rule's *name* and this owns its
//! body, and both produce the same [`Finding`], which renders in the same panel.
//!
//! The check is the one `skills/chapter-canon-check` says goes wrong most often: **a
//! named character doing something a dead person cannot do.** The prose names someone;
//! the scene has a date; did they exist then?
//!
//! Two refusals, both deliberate, both preventing a class of false positive:
//!
//! - **A vague overlap is never a contradiction.** If any reading of the fuzzy dates lets
//!   them coexist, the finding is `Possible` — the world genuinely does not settle the
//!   question, and the prose is choosing a reading of canon rather than breaking it.
//! - **A scene at somebody's own boundary is exempt from it.** A scene depicting a death
//!   sits exactly at the end of that life and would otherwise flag every time. This is
//!   `wb-check`'s `bounded_by` rule, in the form scenes take.

use wb_check::{Certainty, Finding, Rule};
use wb_core::{FuzzyInterval, Interval};
use wb_store::World;

use crate::Story;

/// Findings from the prose, to be merged with the ones from the records.
pub fn check(world: &World, story: &Story) -> Vec<Finding> {
    let mut out = Vec::new();

    for read in &story.reads {
        let Some(scene) = world.scenes.get(&read.scene) else { continue };
        let Some(resolved) = world.resolved_node(&scene.id) else { continue };
        // A scene with no position on the timeline cannot contradict anybody's dates.
        if resolved.nominal.is_none() || (resolved.earliest.is_none() && resolved.latest.is_none())
        {
            continue;
        }
        let extent = Interval::inclusive(resolved.earliest, resolved.latest);
        let window = FuzzyInterval { certain: extent, possible: extent };

        for id in crate::mentions::distinct(&read.mentions) {
            let Some(life) = world.lifespan(&id) else { continue };
            let Some(entity) = world.entities.get(&id) else { continue };

            // The scene *is* the boundary: a chapter that depicts a founding or a death
            // is dated to it, and flagging that would flag the most ordinary shape a
            // story has.
            if entity.existence.as_ref().is_some_and(|span| {
                span.from.depends_on() == Some(scene.id.as_str())
                    || span.to.depends_on() == Some(scene.id.as_str())
            }) {
                continue;
            }

            if life.possible.is_empty() {
                continue;
            }
            let certainty = if !life.possible.overlaps(&window.possible) {
                Certainty::Definite
            } else if life.certain.covers(&window.certain) {
                continue;
            } else {
                Certainty::Possible
            };

            let when = |iv: &Interval| match (iv.from, iv.to) {
                (Some(a), Some(b)) if b.0 - a.0 <= 1 => world.calendar.format_long(a),
                (Some(a), Some(b)) => format!(
                    "{} to {}",
                    world.calendar.format_long(a),
                    world.calendar.format_long(b.offset(-1))
                ),
                (Some(a), None) => format!("{} onwards", world.calendar.format_long(a)),
                (None, Some(b)) => format!("up to {}", world.calendar.format_long(b.offset(-1))),
                (None, None) => "all of time".to_string(),
            };

            let message = match certainty {
                Certainty::Definite => format!(
                    "“{}” names {} on the page, but the scene is set {} and {} existed only {}.",
                    scene.name,
                    entity.name,
                    when(&extent),
                    entity.name,
                    when(&life.possible)
                ),
                Certainty::Possible => format!(
                    "“{}” names {} on the page at {}, which the world permits but does not \
                     confirm — {} certainly existed {}, possibly {}.",
                    scene.name,
                    entity.name,
                    when(&extent),
                    entity.name,
                    when(&life.certain),
                    when(&life.possible)
                ),
            };

            out.push(Finding {
                rule: Rule::SceneContradiction,
                certainty,
                subject: scene.id.clone(),
                related: vec![id.clone()],
                message,
                at: resolved.nominal,
                sources: vec![scene.source.clone(), entity.source.clone()],
            });
        }
    }

    out
}
