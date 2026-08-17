//! What a proposal would do to the world's consistency.
//!
//! This is what makes a review queue worth more than a list of pending edits: before
//! accepting anything, a writer can see which contradictions it settles and which it
//! creates. The check engine already answers both; the only work here is the diff.

use serde::Serialize;
use wb_check::Finding;
use wb_store::World;

use crate::apply::simulate;
use crate::error::Result;
use crate::model::Proposal;

/// `Default` is the empty verdict — nothing settled, nothing broken. Used by the
/// paths that legitimately have no consistency question to ask, such as moving a
/// marker: no rule reads geometry, so running the check would imply one might.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Impact {
    /// Findings the proposal clears.
    pub resolved: Vec<Finding>,
    /// Findings the proposal creates. Definite ones here are a reason to reject.
    pub introduced: Vec<Finding>,
    /// `(definite, possible)` before and after.
    pub before: (usize, usize),
    pub after: (usize, usize),
}

impl Impact {
    pub fn is_neutral(&self) -> bool {
        self.resolved.is_empty() && self.introduced.is_empty()
    }

    /// True when accepting would add a contradiction that is wrong under every reading.
    pub fn breaks_something(&self) -> bool {
        self.introduced.iter().any(|f| f.certainty == wb_check::Certainty::Definite)
    }
}

/// Findings are matched on what they are *about*, not on their wording — the message
/// embeds dates, and a proposal that shifts a date would otherwise look like it had
/// resolved one problem and introduced an identical one.
fn identity(finding: &Finding) -> String {
    format!("{:?}|{}|{}", finding.rule, finding.subject, finding.related.join(","))
}

pub fn impact(world: &World, proposal: &Proposal) -> Result<Impact> {
    Ok(impact_between(world, &simulate(world, proposal)?))
}

/// The same comparison, between two worlds that already exist.
///
/// The app's own edits are not proposals and never become one, but "what does this
/// settle, what does it break" is the same question and deserves the same answer — so
/// the arithmetic lives here rather than being reimplemented against a fake proposal.
pub fn impact_between(world: &World, edited: &World) -> Impact {
    // The merged report, so "what does this settle" includes contradictions found in the
    // prose. Re-dating an event moves every scene anchored to it; checking only the
    // records would tell a reviewer a change breaks nothing while it breaks a chapter.
    let before = wb_story::check(world);
    let after = wb_story::check(edited);

    let before_keys: Vec<String> = before.findings.iter().map(identity).collect();
    let after_keys: Vec<String> = after.findings.iter().map(identity).collect();

    let resolved = before
        .findings
        .iter()
        .zip(&before_keys)
        .filter(|(_, key)| !after_keys.contains(key))
        .map(|(finding, _)| finding.clone())
        .collect();

    let introduced = after
        .findings
        .iter()
        .zip(&after_keys)
        .filter(|(_, key)| !before_keys.contains(key))
        .map(|(finding, _)| finding.clone())
        .collect();

    Impact { resolved, introduced, before: before.counts(), after: after.counts() }
}
