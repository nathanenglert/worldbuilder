//! Everything the pipeline reads, and nothing it writes.
//!
//! These are the *canon* half of terrain: a few dozen numbers a writer owns, versioned
//! in `world.yaml` beside the map image. The terrain itself — coastline, cells, rivers,
//! biomes — is derived from them and never committed, because it can always be rebuilt.
//!
//! Every field has a default that produces a plausible world, so `map: { image: ... }`
//! alone is a complete specification.

use serde::{Deserialize, Deserializer, Serialize};

use crate::rng::Digest;

/// The whole input to [`crate::build`], minus the image itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TerrainParams {
    /// How the sea is told apart from the land.
    pub sea: SeaParams,
    /// Coastline fidelity, `0.0` (blunt) to `1.0` (every pixel). The "detail" slider.
    pub detail: f64,
    /// Target number of substrate cells. Cost is roughly linear in this.
    pub cells: usize,
    /// Seeds cell placement and terrain noise. Change it for a different world at the
    /// same coastline; keep it to make every rebuild identical.
    pub seed: u64,
    pub height: HeightParams,
    pub climate: ClimateParams,
    pub rivers: RiverParams,
}

impl Default for TerrainParams {
    fn default() -> Self {
        Self {
            sea: SeaParams::default(),
            detail: 0.55,
            cells: 2600,
            seed: 1,
            height: HeightParams::default(),
            climate: ClimateParams::default(),
            rivers: RiverParams::default(),
        }
    }
}

impl TerrainParams {
    /// A stable fingerprint. Feeding the image bytes in alongside this is what makes a
    /// cached terrain safe to reuse.
    pub fn digest(&self) -> u64 {
        let mut d = Digest::new();
        d.u64(2); // schema version — bump to invalidate every cache at once
        self.sea.digest(&mut d);
        d.f64(self.detail).u64(self.cells as u64).u64(self.seed);
        self.height.digest(&mut d);
        self.climate.digest(&mut d);
        self.rivers.digest(&mut d);
        d.finish()
    }
}

/// Stage 2 of the pipeline: land/sea segmentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SeaParams {
    /// The colour the writer picked as "this is water".
    #[serde(with = "hex")]
    pub color: [u8; 3],
    /// How far from that colour still counts, `0.0` to `1.0` of the RGB cube's diagonal.
    pub tolerance: f64,
    /// When set, only water reachable from the image border is sea. Enclosed water stays
    /// land in the mask and comes back later as a lake, which is what it actually is.
    pub flood_from_edge: bool,
    /// Land or sea blobs smaller than this many pixels are absorbed by their surroundings.
    /// Compression artefacts and antialiased text produce a great many of them.
    pub min_blob_px: u32,
}

impl Default for SeaParams {
    fn default() -> Self {
        Self { color: [0x1E, 0x3A, 0x4C], tolerance: 0.16, flood_from_edge: true, min_blob_px: 32 }
    }
}

impl SeaParams {
    fn digest(&self, d: &mut Digest) {
        d.bytes(&self.color)
            .f64(self.tolerance)
            .u64(u64::from(self.flood_from_edge))
            .u64(u64::from(self.min_blob_px));
    }
}

/// Stage 6: elevation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HeightParams {
    /// Where the shore sits on the `0.0..=1.0` height scale. Land is above it.
    pub sea_level: f32,
    /// How quickly land rises away from the coast, in normalized map units. Small values
    /// give cliffs; large values give a broad continental shelf.
    pub shelf: f64,
    /// Amplitude of the fractal noise laid over the falloff.
    pub roughness: f64,
    /// Hand-placed mountains. This is the "painted by hand" half of stage 6 — a writer
    /// who knows where the range goes says so, rather than hunting for a seed.
    pub ranges: Vec<Range>,
}

impl Default for HeightParams {
    fn default() -> Self {
        Self { sea_level: 0.32, shelf: 0.12, roughness: 0.22, ranges: Vec::new() }
    }
}

impl HeightParams {
    fn digest(&self, d: &mut Digest) {
        d.f32(self.sea_level).f64(self.shelf).f64(self.roughness).u64(self.ranges.len() as u64);
        for r in &self.ranges {
            d.str(&r.name)
                .f64(r.from[0])
                .f64(r.from[1])
                .f64(r.to[0])
                .f64(r.to[1])
                .f32(r.peak)
                .f64(r.width);
        }
    }
}

/// A mountain range, drawn as a line with a width — the way anyone sketches one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Range {
    #[serde(default)]
    pub name: String,
    /// Normalized map coordinates, the same space as an entity's `marker`.
    pub from: [f64; 2],
    pub to: [f64; 2],
    /// Height at the ridge line, on the same `0.0..=1.0` scale as [`HeightParams`].
    pub peak: f32,
    /// Half-width of the uplift, in normalized units.
    pub width: f64,
}

/// Stage 7: temperature and rainfall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClimateParams {
    /// Normalized `y` of the equator. Values outside `0..1` are the usual case: most
    /// fantasy maps are a corner of a world, not the whole of one.
    pub equator: f64,
    /// Normalized `y` distance from the equator to the pole. Sets how much climate the
    /// map spans — a small number means the map covers many latitudes.
    pub pole_distance: f64,
    pub equator_temp: f32,
    pub pole_temp: f32,
    /// Cooling per full unit of height above sea level, in degrees.
    pub lapse: f32,
    /// Where the prevailing wind blows *from*, in compass degrees. 270 is a westerly.
    pub wind_bearing: f64,
    /// How much moisture open sea hands to the air passing over it, per cell.
    pub evaporation: f32,
    /// Rain wrung out per unit of height gained. This is what makes a rain shadow.
    pub orographic: f32,
    /// Rain that falls over land regardless of terrain, per cell, as a fraction of the
    /// moisture carried.
    pub base_rain: f32,
}

impl Default for ClimateParams {
    fn default() -> Self {
        Self {
            equator: 1.6,
            pole_distance: 2.2,
            equator_temp: 30.0,
            pole_temp: -20.0,
            lapse: 34.0,
            wind_bearing: 260.0,
            evaporation: 0.09,
            orographic: 2.6,
            base_rain: 0.035,
        }
    }
}

impl ClimateParams {
    fn digest(&self, d: &mut Digest) {
        d.f64(self.equator)
            .f64(self.pole_distance)
            .f32(self.equator_temp)
            .f32(self.pole_temp)
            .f32(self.lapse)
            .f64(self.wind_bearing)
            .f32(self.evaporation)
            .f32(self.orographic)
            .f32(self.base_rain);
    }
}

/// Stage 8: drainage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RiverParams {
    /// Accumulated flux, as a fraction of the whole map's rainfall, at which a channel
    /// becomes a river worth drawing.
    pub threshold: f32,
    /// Depressions shallower than this fill into lakes; deeper ones are treated as real
    /// basins with no outlet.
    pub max_lake_depth: f32,
}

impl Default for RiverParams {
    fn default() -> Self {
        Self { threshold: 0.006, max_lake_depth: 0.25 }
    }
}

impl RiverParams {
    fn digest(&self, d: &mut Digest) {
        d.f32(self.threshold).f32(self.max_lake_depth);
    }
}

/// `"#1E3A4C"` on disk, `[u8; 3]` in memory. Writers pick colours in a colour picker.
mod hex {
    use super::{Deserialize, Deserializer, Serialize};

    pub fn serialize<S: serde::Serializer>(v: &[u8; 3], s: S) -> Result<S::Ok, S::Error> {
        format!("#{:02X}{:02X}{:02X}", v[0], v[1], v[2]).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 3], D::Error> {
        let raw = String::deserialize(d)?;
        let body = raw.strip_prefix('#').unwrap_or(&raw);
        let expand = |c: char| -> Option<u8> { c.to_digit(16).map(|d| (d * 17) as u8) };

        let rgb = match body.len() {
            3 => {
                let mut cs = body.chars();
                let mut next = || cs.next().and_then(expand);
                (next(), next(), next())
            }
            6 => {
                let byte = |i: usize| u8::from_str_radix(&body[i..i + 2], 16).ok();
                (byte(0), byte(2), byte(4))
            }
            _ => (None, None, None),
        };

        match rgb {
            (Some(r), Some(g), Some(b)) => Ok([r, g, b]),
            _ => Err(serde::de::Error::custom(format!(
                "{raw:?} is not a colour — expected #RGB or #RRGGBB"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Wrap {
        #[serde(with = "hex")]
        color: [u8; 3],
    }

    fn parse(src: &str) -> Result<[u8; 3], String> {
        serde_yaml_bw::from_str::<Wrap>(src).map(|w| w.color).map_err(|e| e.to_string())
    }

    #[test]
    fn colours_parse_in_both_lengths() {
        assert_eq!(parse("color: '#1E3A4C'").unwrap(), [0x1E, 0x3A, 0x4C]);
        assert_eq!(parse("color: '#2af'").unwrap(), [0x22, 0xAA, 0xFF]);
        assert_eq!(parse("color: 1E3A4C").unwrap(), [0x1E, 0x3A, 0x4C], "the hash is optional");
    }

    #[test]
    fn a_colour_that_is_not_one_says_so() {
        let err = parse("color: '#12345'").unwrap_err();
        assert!(err.contains("not a colour"), "unhelpful: {err}");
        assert!(parse("color: '#zzzzzz'").is_err());
    }

    #[test]
    fn defaults_alone_are_a_complete_specification() {
        let p: TerrainParams = serde_yaml_bw::from_str("{}").unwrap();
        assert_eq!(p, TerrainParams::default());
    }

    #[test]
    fn a_misspelt_field_is_an_error_rather_than_a_silent_default() {
        // The failure mode this prevents: `sealevel: 0.5` quietly doing nothing, and the
        // writer concluding the slider is broken.
        let err = serde_yaml_bw::from_str::<TerrainParams>("height: { sealevel: 0.5 }").unwrap_err();
        assert!(err.to_string().contains("sealevel"), "{err}");
    }

    #[test]
    fn the_digest_moves_when_any_field_does() {
        let base = TerrainParams::default();
        let mut fuzzed = base.clone();
        fuzzed.height.ranges.push(Range {
            name: "Rimefall".into(),
            from: [0.1, 0.2],
            to: [0.3, 0.4],
            peak: 0.9,
            width: 0.1,
        });
        assert_ne!(base.digest(), fuzzed.digest());

        let mut renamed = fuzzed.clone();
        renamed.height.ranges[0].name = "Rimefell".into();
        assert_ne!(fuzzed.digest(), renamed.digest(), "even the name is part of the world");
    }

    #[test]
    fn the_digest_is_stable_across_runs() {
        assert_eq!(TerrainParams::default().digest(), TerrainParams::default().digest());
    }
}
