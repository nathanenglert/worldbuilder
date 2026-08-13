//! **wb-check** — deterministic consistency rules over a world.
//!
//! The design claim this crate exists to make good on: *most consistency checking is
//! not AI*. "Aldric died in 811 but attends the Council of 814" is an interval
//! containment test. So are overlapping territory claims, orphaned references, and a
//! child born before their parent. All of it runs instantly, offline, and cannot
//! hallucinate a contradiction that is not in the data.
//!
//! Every finding carries a [`Certainty`]. `Definite` means no reading of any fuzzy date
//! rescues it. `Possible` means the world's own vagueness leaves room — which is the
//! shape a deliberate mystery takes, and the writer decides which it is. Judgement is
//! where an agent starts being useful; detection is not.
//!
//! Two checks from the design live earlier in the pipeline and are deliberately absent
//! here: anchor cycles and impossible calendar dates both fail at load, because a world
//! that cannot resolve its own dates cannot be queried at all.
//!
//! ```no_run
//! let world = wb_store::load("examples/vashen").unwrap();
//! let report = wb_check::check(&world);
//!
//! for finding in report.definite() {
//!     println!("[{}] {}", finding.rule.slug(), finding.message);
//! }
//! ```

pub mod finding;
mod rules;

pub use finding::{Certainty, Finding, Report, Rule};

use wb_store::World;

/// Run every rule. Findings come back most-serious first: definite before possible,
/// then grouped by rule so a writer fixes one kind of problem at a time.
pub fn check(world: &World) -> Report {
    let mut findings = Vec::new();

    rules::existence_violations(world, &mut findings);
    rules::anachronistic_facts(world, &mut findings);
    rules::conflicting_facts(world, &mut findings);
    rules::orphan_references(world, &mut findings);
    rules::succession_gaps(world, &mut findings);
    rules::impossible_parentage(world, &mut findings);

    findings.sort_by(|a, b| {
        b.certainty
            .cmp(&a.certainty)
            .then(a.rule.cmp(&b.rule))
            .then(a.subject.cmp(&b.subject))
            .then(a.message.cmp(&b.message))
    });

    Report { findings }
}
