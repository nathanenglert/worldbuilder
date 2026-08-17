//! What of a world reaches the page, from the terminal.
//!
//! ```text
//! cargo run -p worldbuilder --example iceberg
//! cargo run -p worldbuilder --example iceberg -- path/to/world
//! cargo run -p worldbuilder --example iceberg -- path/to/world --mentions
//! ```
//!
//! This exists for the same reason `--example terrain` printed an ASCII plot: the risky
//! part of the feature is a *number*, and a number has to be checkable somewhere cheaper
//! than a UI. A bad alias list shows up here as a name in the wrong column, three seconds
//! after editing a record — not as a panel that quietly reads 8% too high.
//!
//! `--mentions` prints the sentence behind every hit, which is the answer to "why does it
//! think that". Nothing in the report is meant to be taken on trust.
//!
//! Always exits zero. An iceberg is a report about where to spend the next hour, not a
//! standard to pass — `--example check` is the one that fails a build.

use std::path::PathBuf;

use wb_store::load;
use wb_story::iceberg::{self, Quadrant};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut root: Option<PathBuf> = None;
    let mut show_mentions = false;

    for arg in args.by_ref() {
        match arg.as_str() {
            "--mentions" => show_mentions = true,
            other => root = Some(PathBuf::from(other)),
        }
    }

    let root = root
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/vashen"));

    let world = match load(&root) {
        Ok(world) => world,
        Err(e) => {
            eprintln!("could not load {}: {e}", root.display());
            std::process::exit(2);
        }
    };

    let story = wb_story::Story::read(&world);
    let report = iceberg::report(&world, &story);

    match report.ratio() {
        Some(pct) => println!(
            "{} — {} of {} records reach the page ({pct}%)",
            world.name, report.surfaced, report.total
        ),
        None => println!("{} — nothing in it yet", world.name),
    }

    let (read, missing) = story.counts();
    match story.standing(&world) {
        wb_story::Standing::Unlinked => {
            println!("no manuscript linked — the whole world is below the waterline, which is");
            println!("a true answer and not a problem. Add `manuscript.root` to world.yaml.\n");
        }
        wb_story::Standing::RootMissing => {
            println!("the manuscript root in world.yaml is not there. Nothing was read.\n");
        }
        wb_story::Standing::Linked => println!("{read} scenes read, {missing} unreadable\n"),
    }

    for (quadrant, heading, note) in [
        (Quadrant::Underbuilt, "UNDERBUILT", "the story leans here and there is little to lean on"),
        (Quadrant::LoadBearing, "LOAD-BEARING", "the spine — know before you change it"),
        (Quadrant::Overbuilt, "BELOW THE WATERLINE", "built, unseen, and doing its job"),
        (Quadrant::Quiet, "QUIET", "stubs, and stubs are not debt"),
    ] {
        let group: Vec<_> = report.of(quadrant).collect();
        if group.is_empty() {
            // Said rather than omitted, and only for the row that matters. An empty
            // underbuilt column is the best news this report can carry, and a section
            // that silently disappears reads as one that failed to run.
            if quadrant == Quadrant::Underbuilt && report.scenes_read > 0 {
                println!("Nothing is underbuilt — the story is not reaching for anything");
                println!("that is not there.\n");
            }
            continue;
        }
        println!("{heading} — {note}");
        for e in group {
            println!(
                "  {:<24} {:>3} mentions  {:>2} scenes  ·  {} facts, {} bytes of prose",
                truncate(&e.name, 24),
                e.mentions,
                e.scenes.len(),
                e.facts,
                e.prose_bytes
            );
            if show_mentions && let Some(line) = &e.first_seen {
                println!("      “{}”", truncate(line, 88));
            }
        }
        println!();
    }

    if !report.unreadable.is_empty() {
        println!("LINKS THAT WENT NOWHERE");
        for (scene, why) in &report.unreadable {
            println!("  {scene}: {why}");
        }
        println!();
    }

    let findings = wb_story::canon::check(&world, &story);
    if findings.is_empty() {
        println!("The prose does not contradict the world.");
    } else {
        println!("THE PAGE AGAINST THE WORLD");
        for f in &findings {
            println!("  [{}] {}", f.certainty.slug(), f.message);
        }
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars().take(width.saturating_sub(1)).collect::<String>() + "…"
}
