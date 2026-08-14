//! Stage 9 — biomes, by a Whittaker-style lookup on temperature and rainfall.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Biome {
    Ocean,
    Shelf,
    Lake,
    Glacier,
    Tundra,
    Taiga,
    ColdDesert,
    TemperateGrassland,
    Shrubland,
    TemperateForest,
    TemperateRainforest,
    Desert,
    Savanna,
    TropicalSeasonalForest,
    TropicalRainforest,
    Alpine,
}

impl Biome {
    pub const ALL: [Biome; 16] = [
        Biome::Ocean,
        Biome::Shelf,
        Biome::Lake,
        Biome::Glacier,
        Biome::Tundra,
        Biome::Taiga,
        Biome::ColdDesert,
        Biome::TemperateGrassland,
        Biome::Shrubland,
        Biome::TemperateForest,
        Biome::TemperateRainforest,
        Biome::Desert,
        Biome::Savanna,
        Biome::TropicalSeasonalForest,
        Biome::TropicalRainforest,
        Biome::Alpine,
    ];

    /// The name a person would use. Goes in the legend and in the MCP payload, so an
    /// agent asked "what is it like there" has a word for it.
    pub fn label(self) -> &'static str {
        match self {
            Biome::Ocean => "ocean",
            Biome::Shelf => "shallows",
            Biome::Lake => "lake",
            Biome::Glacier => "ice",
            Biome::Tundra => "tundra",
            Biome::Taiga => "taiga",
            Biome::ColdDesert => "cold desert",
            Biome::TemperateGrassland => "grassland",
            Biome::Shrubland => "shrubland",
            Biome::TemperateForest => "temperate forest",
            Biome::TemperateRainforest => "temperate rainforest",
            Biome::Desert => "desert",
            Biome::Savanna => "savanna",
            Biome::TropicalSeasonalForest => "seasonal forest",
            Biome::TropicalRainforest => "rainforest",
            Biome::Alpine => "alpine",
        }
    }

    /// Muted enough to sit *under* the political layer without fighting it. The map's
    /// subject is who holds the ground, not what grows on it.
    pub fn color(self) -> &'static str {
        match self {
            Biome::Ocean => "#2A4256",
            Biome::Shelf => "#3B5B70",
            Biome::Lake => "#43708B",
            Biome::Glacier => "#DCE3E6",
            Biome::Tundra => "#9AA69B",
            Biome::Taiga => "#5B7360",
            Biome::ColdDesert => "#A9A490",
            Biome::TemperateGrassland => "#A8B075",
            Biome::Shrubland => "#96A06B",
            Biome::TemperateForest => "#6B8A5A",
            Biome::TemperateRainforest => "#4F7A55",
            Biome::Desert => "#CDBE94",
            Biome::Savanna => "#B9AE6B",
            Biome::TropicalSeasonalForest => "#77955A",
            Biome::TropicalRainforest => "#4C7A48",
            Biome::Alpine => "#8C8C8F",
        }
    }

    pub fn is_water(self) -> bool {
        matches!(self, Biome::Ocean | Biome::Shelf | Biome::Lake)
    }
}

/// Above this height, climate stops deciding and altitude does.
pub const ALPINE: f32 = 0.82;

/// Sea colder than this carries ice on its surface.
const SEA_FREEZE: f32 = -4.0;

/// Land colder than this is under permanent ice, however much it rains.
const LAND_FREEZE: f32 = -8.0;

/// Fraction of the water column, measured down from `sea_level`, that counts as shallow.
const SHELF_DEPTH: f32 = 1.0 / 3.0;

/// Lower edge of each temperature band of [`WHITTAKER`], in degrees Celsius. Anything
/// colder than the first edge falls in row 0.
const TEMPERATURE_BANDS: [f32; 5] = [-8.0, 0.0, 7.0, 15.0, 22.0];

/// Lower edge of each precipitation band of [`WHITTAKER`], normalized.
const PRECIPITATION_BANDS: [f32; 4] = [0.08, 0.2, 0.4, 0.65];

/// The whole climate space at a glance: rows warm downward, columns wet rightward.
///
/// Laid out as a table rather than a chain of `if`s so that every threshold is visible in
/// one place — the failure this guards against is a plausible-looking rule that puts a
/// desert in the wet tropics, which no single branch reads as wrong.
///
/// Row 0 sits below [`LAND_FREEZE`] and so is unreachable through [`classify`]; it is kept
/// because the rows must line up with [`TEMPERATURE_BANDS`], and it holds what the coldest
/// unfrozen ground holds.
const WHITTAKER: [[Biome; 5]; 6] = {
    use Biome::*;
    [
        [Tundra, Tundra, Tundra, Taiga, Taiga],
        [Tundra, Tundra, Tundra, Taiga, Taiga],
        [ColdDesert, TemperateGrassland, TemperateGrassland, Taiga, TemperateForest],
        [ColdDesert, Shrubland, TemperateGrassland, TemperateForest, TemperateRainforest],
        [Desert, Desert, Shrubland, Savanna, TropicalSeasonalForest],
        [Desert, Desert, Savanna, TropicalSeasonalForest, TropicalRainforest],
    ]
};

/// Which band `v` falls in: the number of edges at or below it.
///
/// A NaN compares false against every edge and so lands in the lowest band. That keeps
/// classification total — a cell with no climate still gets a name instead of a panic.
fn band(v: f32, edges: &[f32]) -> usize {
    edges.iter().filter(|e| v >= **e).count()
}

/// Classify every cell.
///
/// Water first: a lake is a [`Biome::Lake`], sea shallower than a third of the way down
/// to the abyss is [`Biome::Shelf`], the rest is [`Biome::Ocean`]. Freezing water at the
/// surface is [`Biome::Glacier`].
///
/// Land goes through a Whittaker lookup on temperature and rainfall — cold and dry is
/// tundra, hot and wet is rainforest, and the interesting middle is grassland, shrubland
/// and forest. Two overrides sit on top: land above [`ALPINE`] is [`Biome::Alpine`]
/// regardless of climate, and land below freezing year-round is [`Biome::Glacier`].
///
/// The boundaries are thresholds rather than a smooth field, deliberately. A writer wants
/// to be told "this is savanna", not handed a probability distribution.
///
/// The lookup itself is [`WHITTAKER`], indexed by [`TEMPERATURE_BANDS`] then
/// [`PRECIPITATION_BANDS`]. Every input is read pointwise, so the result depends on
/// nothing but the cell's own numbers.
pub fn classify(
    temperature: &[f32],
    precipitation: &[f32],
    heights: &[f32],
    is_land: &[bool],
    lake: &[bool],
    sea_level: f32,
) -> Vec<Biome> {
    let shelf_floor = sea_level * (1.0 - SHELF_DEPTH);

    (0..is_land.len())
        .map(|i| {
            let (t, p, h) = (temperature[i], precipitation[i], heights[i]);

            if lake[i] {
                // A lake outranks everything: it is water the drainage stage put there,
                // and no amount of climate makes it dry ground.
                Biome::Lake
            } else if !is_land[i] {
                if t < SEA_FREEZE {
                    Biome::Glacier
                } else if h >= shelf_floor {
                    Biome::Shelf
                } else {
                    Biome::Ocean
                }
            } else if h > ALPINE {
                Biome::Alpine
            } else if t < LAND_FREEZE {
                Biome::Glacier
            } else {
                WHITTAKER[band(t, &TEMPERATURE_BANDS)][band(p, &PRECIPITATION_BANDS)]
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEA_LEVEL: f32 = 0.32;

    /// One cell, classified.
    fn cell(t: f32, p: f32, h: f32, is_land: bool, lake: bool) -> Biome {
        classify(&[t], &[p], &[h], &[is_land], &[lake], SEA_LEVEL)[0]
    }

    /// One ordinary land cell — well above the shore, well below the alpine line.
    fn land(t: f32, p: f32) -> Biome {
        cell(t, p, 0.5, true, false)
    }

    /// How much water the biome wants, driest first. Only the biomes [`WHITTAKER`] can
    /// produce have a rank; anything else reaching here is the bug the test is hunting.
    fn aridity(b: Biome) -> u8 {
        match b {
            Biome::Desert | Biome::ColdDesert => 0,
            Biome::Tundra => 1,
            Biome::Shrubland => 2,
            Biome::TemperateGrassland | Biome::Savanna => 3,
            Biome::Taiga => 4,
            Biome::TemperateForest | Biome::TropicalSeasonalForest => 5,
            Biome::TemperateRainforest | Biome::TropicalRainforest => 6,
            other => panic!("{} is not a climate biome", other.label()),
        }
    }

    #[test]
    fn the_four_corners_of_the_climate_space_are_the_ones_everyone_expects() {
        assert_eq!(land(-5.0, 0.0), Biome::Tundra, "cold and dry");
        assert_eq!(land(-5.0, 1.0), Biome::Taiga, "cold and wet");
        assert_eq!(land(35.0, 0.0), Biome::Desert, "hot and dry");
        assert_eq!(land(35.0, 1.0), Biome::TropicalRainforest, "hot and wet");
    }

    #[test]
    fn warm_and_moderately_wet_is_not_a_desert() {
        for p in [0.25, 0.3, 0.35, 0.5, 0.8] {
            assert_ne!(land(18.0, p), Biome::Desert, "at {p} rain");
        }
    }

    #[test]
    fn wetting_a_climate_never_makes_the_biome_drier() {
        // Everything from the freezing point of ground up past the tropics. A table
        // indexed the wrong way round shows up here as rainfall making a place arid.
        for step in 0..=44 {
            let t = LAND_FREEZE + step as f32;
            let mut driest = 0;
            for j in 0..=100 {
                let p = j as f32 / 100.0;
                let b = land(t, p);
                let rank = aridity(b);
                assert!(rank >= driest, "at {t}C, {p} rain dried out into {}", b.label());
                driest = rank;
            }
        }
    }

    #[test]
    fn every_row_of_the_lookup_wets_monotonically() {
        for (row, precipitations) in WHITTAKER.iter().enumerate() {
            for pair in precipitations.windows(2) {
                let (dry, wet) = (pair[0], pair[1]);
                assert!(
                    aridity(dry) <= aridity(wet),
                    "row {row}: {} sits left of the drier {}",
                    dry.label(),
                    wet.label()
                );
            }
        }
    }

    #[test]
    fn a_lake_is_a_lake_whatever_else_would_have_claimed_it() {
        assert_eq!(cell(-30.0, 0.0, 0.5, true, true), Biome::Lake, "frozen solid");
        assert_eq!(cell(35.0, 1.0, 0.95, true, true), Biome::Lake, "above the alpine line");
        assert_eq!(cell(-30.0, 0.0, 0.05, false, true), Biome::Lake, "flagged over deep sea");
    }

    #[test]
    fn a_sea_cell_never_gets_a_land_biome_nor_a_land_cell_a_sea_one() {
        let (mut temperature, mut precipitation, mut heights, mut is_land) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for wet in [false, true] {
            for t in [-40.0, -20.0, -8.0, -4.0, 0.0, 7.0, 15.0, 22.0, 30.0, 45.0] {
                for p in [0.0, 0.08, 0.2, 0.4, 0.65, 1.0] {
                    for h in [0.0, 0.05, 0.21, 0.22, 0.32, 0.6, 0.82, 0.83, 1.0] {
                        temperature.push(t);
                        precipitation.push(p);
                        heights.push(h);
                        is_land.push(!wet);
                    }
                }
            }
        }
        let lake = vec![false; is_land.len()];
        let out = classify(&temperature, &precipitation, &heights, &is_land, &lake, SEA_LEVEL);

        for (i, b) in out.iter().enumerate() {
            if is_land[i] {
                assert!(!b.is_water(), "land cell {i} came out as {}", b.label());
            } else {
                assert!(
                    matches!(b, Biome::Ocean | Biome::Shelf | Biome::Glacier),
                    "sea cell {i} came out as {}",
                    b.label()
                );
            }
        }
    }

    #[test]
    fn the_shelf_is_the_shallowest_third_of_the_water_column() {
        // A third of the way from a 0.32 shore down to the abyss is 0.213.
        assert_eq!(cell(10.0, 0.5, 0.31, false, false), Biome::Shelf, "just off the beach");
        assert_eq!(cell(10.0, 0.5, 0.22, false, false), Biome::Shelf, "just inside the edge");
        assert_eq!(cell(10.0, 0.5, 0.21, false, false), Biome::Ocean, "just outside it");
        assert_eq!(cell(10.0, 0.5, 0.0, false, false), Biome::Ocean, "the abyss");
    }

    #[test]
    fn the_sea_freezes_at_a_warmer_temperature_than_the_land_does() {
        assert_eq!(cell(-5.0, 0.5, 0.3, false, false), Biome::Glacier, "sea ice at -5");
        assert_eq!(land(-5.0, 0.3), Biome::Tundra, "land at -5 is merely cold");
        assert_eq!(land(-12.0, 0.5), Biome::Glacier, "land ice at -12");
        assert_eq!(land(-12.0, 1.0), Biome::Glacier, "rain does not thaw it");
    }

    #[test]
    fn the_alpine_line_overrides_whatever_the_climate_would_say() {
        assert_eq!(cell(35.0, 1.0, 0.9, true, false), Biome::Alpine, "a peak in the tropics");
        assert_eq!(cell(-30.0, 0.0, 0.9, true, false), Biome::Alpine, "and one in the ice");
        assert_eq!(
            cell(35.0, 1.0, ALPINE, true, false),
            Biome::TropicalRainforest,
            "at the line the climate still decides — only above it does altitude"
        );
    }

    #[test]
    fn the_output_has_exactly_one_biome_per_cell() {
        for n in [0, 1, 7, 64] {
            let out = classify(
                &vec![12.0; n],
                &vec![0.5; n],
                &vec![0.4; n],
                &vec![true; n],
                &vec![false; n],
                SEA_LEVEL,
            );
            assert_eq!(out.len(), n);
        }
    }

    #[test]
    fn a_cell_with_no_climate_at_all_is_still_named() {
        // NaN must not escape as a panic, nor as water on dry land.
        let b = land(f32::NAN, f32::NAN);
        assert!(!b.is_water(), "{} is not land", b.label());
        assert_eq!(b, WHITTAKER[0][0], "an unknown climate reads as the coldest, driest corner");
    }
}
