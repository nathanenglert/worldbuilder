//! A world bible, from the terminal.
//!
//! ```text
//! cargo run -p worldbuilder --example export
//! cargo run -p worldbuilder --example export -- examples/vashen --out /tmp/bible.html
//! cargo run -p worldbuilder --example export -- examples/vashen --at 0812-04
//! cargo run -p worldbuilder --example export -- examples/vashen --on-the-page
//! ```
//!
//! Terminal-first, for the third slice running. `--example terrain` tuned the map by
//! printing it as ASCII; `--example iceberg` tuned the mention matcher before a panel
//! existed. The risky part of an exporter is different again — the file *opens* whatever
//! is wrong with it — so this prints the structural checks that a browser will not:
//! how big it came out, how many records went in, and whether every link in it lands.
//!
//! Writing nothing unless `--out` is given, because the default should never be to leave
//! a file behind on somebody's disk.

use std::collections::BTreeSet;
use std::path::PathBuf;

use wb_core::Day;
use wb_export::Scope;
use wb_store::load;

fn main() {
    let mut root: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut at: Option<String> = None;
    let mut on_the_page = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().map(PathBuf::from),
            "--at" => at = args.next(),
            "--on-the-page" => on_the_page = true,
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

    let scope = match (&at, on_the_page) {
        (Some(expr), _) => match world.day_of(expr) {
            Ok(Some(day)) => Scope::AsOf(Day(day.0)),
            Ok(None) => {
                eprintln!("`{expr}` has no position on this world's timeline");
                std::process::exit(2);
            }
            Err(e) => {
                eprintln!("`{expr}` is not a date this world understands: {e}");
                std::process::exit(2);
            }
        },
        (None, true) => Scope::OnThePage,
        _ => Scope::Everything,
    };

    let document = wb_export::bible(&world, scope);

    println!("{} — {}", world.name, scope.slug());
    println!("{:>9} bytes", document.len());
    println!("{:>9} records in the document", count(&document, "<article id="));
    println!(
        "{:>9} of {} records in the world",
        count(&document, "<article id="),
        world.entities.len() + world.events.len()
    );
    println!("{:>9} cross-links", count(&document, "href=\"#"));

    // The failure this file format is prone to: a link that looks fine and lands nowhere.
    // A browser will not tell anybody, so it gets checked here every single run.
    let anchors: BTreeSet<&str> = ids(&document, "id=\"").collect();
    let dangling: Vec<&str> =
        ids(&document, "href=\"#").filter(|target| !anchors.contains(target)).collect();
    if dangling.is_empty() {
        println!("{:>9} every link lands in the same file", "✓");
    } else {
        println!("  DANGLING: {dangling:?}");
    }

    match out {
        Some(path) => match std::fs::write(&path, &document) {
            Ok(()) => println!("\nwritten to {}", path.display()),
            Err(e) => {
                eprintln!("could not write {}: {e}", path.display());
                std::process::exit(2);
            }
        },
        None => println!("\n(nothing written — pass --out to keep it)"),
    }
}

fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

/// Every value of an attribute that opens with `prefix`, up to the closing quote.
fn ids<'a>(text: &'a str, prefix: &'a str) -> impl Iterator<Item = &'a str> {
    text.split(prefix).skip(1).filter_map(|rest| rest.split('"').next())
}
