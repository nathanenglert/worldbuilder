//! `cargo run --release -p wb-mcp --example scale` — what a large world costs.
//!
//! The design has SQLite as a derived index, deferred until there were real query shapes
//! to index *for*. There are now, so this measures the four costs that would justify it:
//! loading at launch, fingerprinting on every call, answering a snapshot, and running the
//! full consistency pass.
//!
//! Sizes are chosen against real projects, not round numbers. A working novelist's world
//! runs to a few hundred records; a World Anvil power user with a decade of material
//! reaches a few thousand. 20,000 is past anything anyone has.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use wb_core::Day;
use wb_store::load;

fn main() {
    let root = std::env::temp_dir().join("wb-scale");
    println!(
        "{:>8}  {:>7}  {:>10}  {:>12}  {:>10}  {:>10}  {:>10}",
        "entities", "events", "load", "fingerprint", "world_at", "search", "check"
    );

    for &n in &[200usize, 1_000, 5_000, 20_000] {
        let dir = root.join(n.to_string());
        generate(&dir, n);

        let start = Instant::now();
        let world = load(&dir).expect("generated world loads");
        let load_ms = start.elapsed().as_secs_f64() * 1e3;

        // The cost this server pays on *every* call to stay honest against the disk.
        // Measured on an unchanged tree, which is the common case: what a query pays to
        // find out it does not need to reload.
        let handle = wb_mcp::WorldHandle::open(&dir).unwrap();
        let stat_ms = time(|| {
            std::hint::black_box(handle.with(|w| w.entities.len()).unwrap());
        });
        assert_eq!(handle.reloads(), 0, "the tree did not change, so nothing should reload");

        let day =
            world.change_points().get(world.change_points().len() / 2).copied().unwrap_or(Day(0));
        let snapshot_ms = time(|| {
            std::hint::black_box(world.at(day).len());
        });
        let search_ms = time(|| {
            std::hint::black_box(world.search("hold", 25).len());
        });
        let check_ms = time(|| {
            std::hint::black_box(wb_check::check(&world).findings.len());
        });

        println!(
            "{n:>8}  {:>7}  {load_ms:>9.1}ms  {stat_ms:>11.2}ms  {snapshot_ms:>9.2}ms  \
             {search_ms:>9.2}ms  {check_ms:>9.1}ms",
            world.events.len()
        );
    }

    let _ = fs::remove_dir_all(&root);
}

/// Median of a handful of runs. Not a statistical benchmark — the question is which
/// order of magnitude each cost sits in, and that survives crude timing.
fn time(mut f: impl FnMut()) -> f64 {
    let mut runs: Vec<f64> = (0..5)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed().as_secs_f64() * 1e3
        })
        .collect();
    runs.sort_by(f64::total_cmp);
    runs[runs.len() / 2]
}

/// A world shaped like a real one: places owned by polities over time, actors with
/// parents and titles, and events the facts anchor to.
fn generate(dir: &Path, entities: usize) {
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir.join("entities")).unwrap();
    fs::create_dir_all(dir.join("events")).unwrap();

    fs::write(
        dir.join("world.yaml"),
        r#"name: Scale Test
calendar:
  name: Test
  months:
    - { name: One, days: 30 }
    - { name: Two, days: 30 }
    - { name: Three, days: 30 }
    - { name: Four, days: 30 }
types:
  - { name: hold, primitive: place }
  - { name: house, primitive: polity }
  - { name: noble, primitive: actor }
"#,
    )
    .unwrap();

    let polities = (entities / 20).max(2);
    let events = (entities / 10).max(2);

    for i in 0..events {
        let year = 100 + i * 3;
        fs::write(
            dir.join(format!("events/e{i}.yaml")),
            format!(
                "id: evt_{i}\nname: Event {i}\nkind: war\ndate: \"{year:04}-02-15\"\n\
                 participants: [pol_{}]\n",
                i % polities
            ),
        )
        .unwrap();
    }

    for i in 0..entities {
        let (path, body): (PathBuf, String) = match i % 20 {
            // Polities: a colour, and a capital that moves once.
            0 => {
                let body = format!(
                    "---\nid: pol_{p}\nname: House {p}\ntype: house\n\
                     existence: {{ from: \"{start:04}\" }}\nfacts:\n\
                     - {{ attr: color, value: \"#3366{p:02X}\" }}\n\
                     - {{ attr: capital, value: place_{a}, from: \"@evt_{e}\" }}\n---\n\n\
                     A house of the interior, holding since the {start}s.\n",
                    p = i / 20,
                    a = i + 1,
                    e = (i / 20) % events,
                    start = 100 + i % 400,
                );
                (dir.join(format!("entities/pol{i}.md")), body)
            }
            // Actors: parentage, and a title held from an event.
            1..=4 => {
                let body = format!(
                    "---\nid: act_{i}\nname: Noble {i}\ntype: noble\n\
                     existence: {{ from: \"{birth:04}~\", to: \"{death:04}~\" }}\n\
                     {parents}facts:\n\
                     - {{ attr: title, value: \"Warden {i}\", from: \"@evt_{e}\" }}\n---\n\n\
                     Fourth of that name, and the last to hold the crossing.\n",
                    birth = 200 + i % 300,
                    death = 260 + i % 300,
                    e = i % events,
                    parents =
                        if i > 40 { format!("parents: [act_{}]\n", i - 20) } else { String::new() },
                );
                (dir.join(format!("entities/act{i}.md")), body)
            }
            // Places: geometry, and ownership that changes hands at an event.
            _ => {
                let owner = i % polities;
                let next = (owner + 1) % polities;
                let body = format!(
                    "---\nid: place_{i}\nname: Hold {i}\ntype: hold\n\
                     existence: {{ from: \"{start:04}\" }}\n\
                     marker: [{x:.3}, {y:.3}]\n\
                     shape:\n  - [{x:.3}, {y:.3}]\n  - [{x2:.3}, {y:.3}]\n  - [{x2:.3}, {y2:.3}]\n\
                     facts:\n\
                     - {{ attr: owner, value: pol_{owner}, from: \"{start:04}\", to: \"@evt_{e}\" }}\n\
                     - {{ attr: owner, value: pol_{next}, from: \"@evt_{e}\" }}\n\
                     - {{ attr: population, value: {pop} }}\n---\n\n\
                     A walled hold on the road, and the last of the old crossings.\n",
                    start = 100 + i % 200,
                    e = i % events,
                    x = (i % 97) as f64 / 97.0,
                    y = (i % 89) as f64 / 89.0,
                    x2 = (i % 97) as f64 / 97.0 + 0.01,
                    y2 = (i % 89) as f64 / 89.0 + 0.01,
                    pop = 500 + i * 7,
                );
                (dir.join(format!("entities/place{i}.md")), body)
            }
        };
        fs::write(path, &body).unwrap();
    }
}
