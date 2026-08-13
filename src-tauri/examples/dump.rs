//! Print the snapshot payload the UI would receive for a given date.
//!
//! ```text
//! cargo run -p worldbuilder --example dump -- 0812-04-15
//! cargo run -p worldbuilder --example dump -- @evt_siege_of_marrow+2y
//! ```

use std::path::PathBuf;

use wb_store::load;
use worldbuilder_lib::commands::SnapshotDto;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/vashen");
    let world = match load(&root) {
        Ok(world) => world,
        Err(e) => {
            eprintln!("could not load {}: {e}", root.display());
            std::process::exit(1);
        }
    };

    let expr = std::env::args().nth(1).unwrap_or_else(|| "0812-04-15".to_string());
    let day = match world.day_of(&expr) {
        Ok(Some(day)) => day,
        Ok(None) => {
            eprintln!("`{expr}` has no position on the timeline");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("could not read `{expr}`: {e}");
            std::process::exit(1);
        }
    };

    let snapshot = SnapshotDto::of(&world, day);
    println!("{}", serde_json::to_string_pretty(&snapshot).expect("serialize snapshot"));
}
