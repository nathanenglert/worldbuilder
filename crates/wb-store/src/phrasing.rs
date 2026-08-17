//! Saying a date to somebody who has never seen this application.
//!
//! Three surfaces need this now — the exported bible, the lineage chart's row labels, and
//! whatever comes next — and all three get the same subtle thing wrong in the same way if
//! they roll their own. Resolving a `DateExpr` to a `Day` and formatting that is the
//! obvious implementation and it is *lossy in the direction this whole tool is about*:
//! the founding of Corrath is dated `0500`, and printing it as `0500-01-01` states a
//! month and a day nobody wrote down. Widening `0602~` by a year of fuzz and printing the
//! near edge is worse still — `0599-12-21` is not even approximately what the record says.
//!
//! So: keep the writer's own precision wherever there is any, and translate only the
//! anchors, because `@evt_oath_of_vashen` is bookkeeping that belongs in the file rather
//! than in front of a reader.

use wb_core::DateExpr;

use crate::world::World;

/// One endpoint, in words a reader who has never seen this application can act on.
///
/// A date keeps the precision the writer gave it. `0500` stays `0500` rather than
/// becoming `0500-01-01`, because printing a day the world never claimed is the one
/// mistake a document about deliberate vagueness cannot make. Only anchors are
/// translated — `@evt_oath_of_vashen` is bookkeeping, and the reader wants the year.
pub fn phrase(world: &World, owner: &str, expr: &DateExpr) -> Option<String> {
    Some(match expr {
        DateExpr::Unknown => return None,
        DateExpr::Civil { date, approx: false } => date.to_string(),
        DateExpr::Civil { date, approx: true } => format!("about {date}"),
        DateExpr::Range { lo, hi } => format!("between {lo} and {hi}"),
        DateExpr::After { date } => format!("after {date}"),
        DateExpr::Before { date } => format!("before {date}"),
        // "At the oath" is not a day, and resolving it to one invents a precision the
        // world never claimed: the oath itself is dated `0806-02-14`, but the founding is
        // dated `0500`, and printing that as `0500-01-01` is a lie the reader cannot
        // detect. So a bare anchor borrows the phrasing of whatever it points at, and
        // only an anchor with an offset — where a day genuinely is being computed — is
        // resolved numerically.
        DateExpr::Anchor { node, offset, approx } if offset.is_zero() && !approx => {
            match borrowed(world, node) {
                Some(phrase) => phrase,
                None => world.calendar.format_numeric(world.resolve_in(owner, expr).ok()?.nominal?),
            }
        }
        DateExpr::Anchor { approx, .. } => {
            let day = world.resolve_in(owner, expr).ok()?.nominal?;
            let label = world.calendar.format_numeric(day);
            if *approx { format!("about {label}") } else { label }
        }
    })
}

/// The date the anchored record states for itself, in its own words.
///
/// One level only, and never through another anchor: a chain of them is exactly where a
/// recursive borrow would loop, and the resolver already has the answer for those.
fn borrowed(world: &World, node: &str) -> Option<String> {
    let (id, end) = match node.split_once('.') {
        Some((id, end)) => (id, Some(end)),
        None => (node, None),
    };

    let expr = match (world.events.get(id), world.scenes.get(id), world.entities.get(id)) {
        (Some(event), _, _) => event.date.clone(),
        (_, Some(scene), _) => scene.date.clone(),
        (_, _, Some(entity)) => {
            let span = entity.existence.as_ref()?;
            match end {
                Some("birth" | "start") => span.from.clone(),
                Some("death" | "end") => span.to.clone(),
                _ => return None,
            }
        }
        _ => return None,
    };

    match expr {
        DateExpr::Anchor { .. } | DateExpr::Unknown => None,
        other => phrase(world, id, &other),
    }
}
