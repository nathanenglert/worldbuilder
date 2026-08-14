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
pub fn classify(
    temperature: &[f32],
    precipitation: &[f32],
    heights: &[f32],
    is_land: &[bool],
    lake: &[bool],
    sea_level: f32,
) -> Vec<Biome> {
    let _ = (temperature, precipitation, heights, is_land, lake, sea_level);
    todo!("stage 9")
}
