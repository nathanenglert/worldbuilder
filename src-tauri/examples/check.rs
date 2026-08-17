//! Run the consistency rules over a world from the terminal.
//!
//! ```text
//! cargo run -p worldbuilder --example check
//! cargo run -p worldbuilder --example check -- path/to/world
//! ```
//!
//! Exits non-zero only for definite findings. Possible ones are questions, not errors,
//! so they never fail a build.

use std::path::PathBuf;

use wb_check::Certainty;
use wb_store::load;

fn main() {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/vashen"));

    let world = match load(&root) {
        Ok(world) => world,
        Err(e) => {
            eprintln!("could not load {}: {e}", root.display());
            std::process::exit(2);
        }
    };

    // `wb_story::check` rather than `wb_check::check`: a contradiction found in the
    // prose is a contradiction, and a build that passes because nobody read the book is
    // not a build that passed.
    let report = wb_story::check(&world);
    let (definite, possible) = report.counts();

    println!("{} — {definite} definite, {possible} possible\n", world.name);

    for (heading, certainty) in
        [("DEFINITE", Certainty::Definite), ("POSSIBLE", Certainty::Possible)]
    {
        let group: Vec<_> = report.findings.iter().filter(|f| f.certainty == certainty).collect();
        if group.is_empty() {
            continue;
        }
        println!("{heading}");
        for finding in group {
            println!("  {} [{}]", finding.message, finding.rule.slug());
            for source in &finding.sources {
                println!("      {}", source.display());
            }
        }
        println!();
    }

    if report.is_clean() {
        println!("Nothing to report.");
    }

    std::process::exit(if definite > 0 { 1 } else { 0 });
}
