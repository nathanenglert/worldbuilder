//! Runs the map pipeline over a world and prints what came out.
//!
//! `cargo run -p worldbuilder --example terrain [-- <world> [--wide N]]`
//!
//! The ASCII plot is not decoration. Every other way of checking this pipeline — a unit
//! test, a statistic — can pass while the map is nonsense: a river running uphill, a
//! desert on the windward coast, an island where the sea should be. Seeing it in eighty
//! columns catches that in a second.

use std::collections::BTreeMap;
use std::path::PathBuf;

use wb_store::load;
use wb_terrain::{Biome, Terrain};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/vashen");
    let mut wide = 96usize;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--wide" => wide = args.next().and_then(|v| v.parse().ok()).unwrap_or(wide),
            other => root = PathBuf::from(other),
        }
    }

    let world = match load(&root) {
        Ok(w) => w,
        Err(e) => return eprintln!("{e}"),
    };

    let start = std::time::Instant::now();
    let terrain = match world.terrain() {
        Ok(Some(t)) => t,
        Ok(None) => return eprintln!("{} declares no map:", world.name),
        Err(e) => return eprintln!("{e}"),
    };
    let elapsed = start.elapsed();

    let s = &terrain.stats;
    println!("{} — {}", world.name, root.display());
    println!(
        "  {} × {} px, aspect {:.3}, sea level {:.2}, built in {:.0} ms",
        terrain.source_width,
        terrain.source_height,
        terrain.aspect,
        terrain.sea_level,
        elapsed.as_secs_f64() * 1000.0
    );
    println!(
        "  {:.1}% land · {} cells · {} islands, {} inlets · {} coast points",
        s.land_fraction * 100.0,
        s.cells,
        s.islands,
        s.inlets,
        s.coast_points
    );
    println!(
        "  {} rivers · {} lake cells · highest {:.2} · {:.1}…{:.1} °C",
        s.rivers, s.lake_cells, s.highest, s.temperature_min, s.temperature_max
    );

    println!("\nbiomes");
    for b in &s.biomes {
        let share = b.cells as f64 / s.cells as f64 * 100.0;
        println!("  {:>5.1}%  {:<22} {:>5}", share, b.label, b.cells);
    }

    println!("\nrivers, longest first");
    let mut rivers: Vec<_> = terrain.rivers.iter().collect();
    rivers.sort_by_key(|r| std::cmp::Reverse(r.cells.len()));
    for r in rivers.iter().take(8) {
        let head = r.points.first().copied().unwrap_or_default();
        let mouth = r.points.last().copied().unwrap_or_default();
        println!(
            "  order {} · {:>3} cells · [{:.2},{:.2}] → [{:.2},{:.2}] into the {:?}",
            r.order,
            r.cells.len(),
            head[0],
            head[1],
            mouth[0],
            mouth[1],
            r.mouth
        );
    }

    println!("\nwhat is under each record");
    for e in world.entities.values() {
        let Some(marker) = e.marker else { continue };
        let Some(p) = terrain.describe(marker) else { continue };
        println!(
            "  {:<22} {:<20} {:>5.0} °C  rain {:>4.2}  {}{}",
            e.name,
            p.biome_label,
            p.temperature,
            p.precipitation,
            if p.on_river { "on a river " } else { "" },
            if p.is_land { "" } else { "IN THE SEA" }
        );
    }

    println!();
    plot(&terrain, wide);
}

/// One character per sampled cell. Water is punctuation, land is letters, and a river
/// overwrites whatever it runs through — so a river that leaves the land is visible.
fn plot(t: &Terrain, wide: usize) {
    let tall = ((wide as f64 / t.aspect) * 0.5).round().max(4.0) as usize;
    let mut grid = vec![vec![' '; wide]; tall];

    for (y, row) in grid.iter_mut().enumerate() {
        for (x, cell) in row.iter_mut().enumerate() {
            let p = [(x as f64 + 0.5) / wide as f64, (y as f64 + 0.5) / tall as f64];
            let Some(i) = t.cell_at(p) else { continue };
            *cell = glyph(t.cells.biome[i]);
        }
    }

    for river in &t.rivers {
        for p in &river.points {
            let x = ((p[0] * wide as f64) as usize).min(wide - 1);
            let y = ((p[1] * tall as f64) as usize).min(tall - 1);
            grid[y][x] = '≈';
        }
    }

    let mut legend: BTreeMap<char, &str> = BTreeMap::new();
    for b in Biome::ALL {
        legend.entry(glyph(b)).or_insert(b.label());
    }

    for row in &grid {
        println!("  {}", row.iter().collect::<String>());
    }
    println!(
        "\n  {}   ≈ river",
        legend.iter().map(|(c, l)| format!("{c} {l}")).collect::<Vec<_>>().join("   ")
    );
}

fn glyph(b: Biome) -> char {
    match b {
        Biome::Ocean => '.',
        Biome::Shelf => ',',
        Biome::Lake => 'o',
        Biome::Glacier => '*',
        Biome::Tundra => 'u',
        Biome::Taiga => 'T',
        Biome::ColdDesert => 'c',
        Biome::TemperateGrassland => 'g',
        Biome::Shrubland => 's',
        Biome::TemperateForest => 'F',
        Biome::TemperateRainforest => 'R',
        Biome::Desert => 'd',
        Biome::Savanna => 'v',
        Biome::TropicalSeasonalForest => 'f',
        Biome::TropicalRainforest => 'J',
        Biome::Alpine => '^',
    }
}
