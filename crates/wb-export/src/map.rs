//! The map, at one instant, as SVG in the page.
//!
//! Everything the app draws that is *derived* — coastline, cells, height, rain, rivers,
//! biomes — is left out. The backdrop is the image the writer drew, and the shapes on top
//! of it are the world's own geometry; that is the layer canon actually lives in, and
//! reproducing eight pipeline stages into a document would double its size to show
//! somebody a rendering they can already get from the application.
//!
//! What is kept is the one thing the README leads with: a claim nobody dated is drawn
//! **hatched** rather than handed to whichever polity the code happened to check first.

use std::collections::BTreeSet;
use wb_core::{Containment, Day};
use wb_store::World;

use crate::html::escape;

/// Standard base64, no wrapping. Twenty lines against a dependency that would be pulled
/// in to encode exactly one PNG per document.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6 & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[(n & 63) as usize] as char } else { '=' });
    }
    out
}

/// Width and height out of a PNG's IHDR chunk.
///
/// Sixteen bytes of a fixed header, rather than a decoder: the aspect ratio is the only
/// thing needed, and the pixels are going into the document untouched as a `data:` URI.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if bytes.len() < 24 || bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let read =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    Some((read(16), read(20)))
}

/// One live claim on a piece of ground.
struct Claim {
    color: String,
    settled: bool,
}

fn claims(world: &World, id: &str, day: Day) -> Vec<Claim> {
    let Some(view) = world.entity_at(id, day) else { return Vec::new() };
    view.facts
        .iter()
        .filter(|f| f.attr == "owner")
        .map(|f| {
            let owner = f.value.to_string();
            Claim {
                color: world
                    .value_at(&owner, "color", day)
                    .map(|c| c.value.to_string())
                    .unwrap_or_else(|| "#7d766b".to_string()),
                settled: f.certainty == Containment::Yes,
            }
        })
        .collect()
}

fn points(shape: &[[f64; 2]], w: f64, h: f64) -> String {
    shape.iter().map(|p| format!("{:.1},{:.1}", p[0] * w, p[1] * h)).collect::<Vec<_>>().join(" ")
}

/// The map as it stood on `day`, or `None` if this world has no map image to stand on.
pub fn figure(world: &World, included: &BTreeSet<String>, day: Day) -> Option<String> {
    let spec = world.map.as_ref()?;
    let bytes = std::fs::read(world.root.join(&spec.image)).ok()?;
    let (pixels_w, pixels_h) = png_size(&bytes)?;

    let w = 1000.0_f64;
    let h = (w * f64::from(pixels_h) / f64::from(pixels_w)).round();

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w:.0} {h:.0}\" role=\"img\" \
         aria-label=\"The world as it stood\" xmlns=\"http://www.w3.org/2000/svg\">\
         <defs><pattern id=\"contested\" width=\"9\" height=\"9\" patternUnits=\"userSpaceOnUse\" \
         patternTransform=\"rotate(45)\">\
         <line x1=\"0\" y1=\"0\" x2=\"0\" y2=\"9\" stroke=\"#7d766b\" stroke-width=\"2.5\" \
         stroke-opacity=\".55\"/></pattern></defs>"
    );
    svg.push_str(&format!(
        "<image href=\"data:image/png;base64,{}\" x=\"0\" y=\"0\" width=\"{w:.0}\" \
         height=\"{h:.0}\" preserveAspectRatio=\"none\"/>",
        base64(&bytes)
    ));

    // Territory first, so a settlement's dot is never buried under a claim.
    for entity in world.entities.values() {
        if entity.shape.len() < 3 || !included.contains(&entity.id) {
            continue;
        }
        let here = claims(world, &entity.id, day);
        if here.is_empty() && world.entity_at(&entity.id, day).is_none() {
            continue;
        }
        let settled: Vec<&Claim> = here.iter().filter(|c| c.settled).collect();
        let fill = match (here.len(), settled.first()) {
            (0, _) => "none".to_string(),
            (1, Some(one)) => one.color.clone(),
            // More than one live claim, or one nobody has settled: the honest drawing is
            // hatching, not a coin toss.
            _ => "url(#contested)".to_string(),
        };
        svg.push_str(&format!(
            "<polygon points=\"{}\" fill=\"{}\" fill-opacity=\"{}\" stroke=\"#1c1a17\" \
             stroke-opacity=\".35\" stroke-width=\"1.5\"{}/>",
            points(&entity.shape, w, h),
            escape(&fill),
            if fill == "none" { "0" } else { "0.42" },
            if here.len() > 1 { " stroke-dasharray=\"7 5\"" } else { "" }
        ));
    }

    for entity in world.entities.values() {
        let Some(marker) = entity.marker else { continue };
        if !included.contains(&entity.id) || world.entity_at(&entity.id, day).is_none() {
            continue;
        }
        let (x, y) = (marker[0] * w, marker[1] * h);
        svg.push_str(&format!(
            "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"5\" fill=\"#1c1a17\" stroke=\"#fbfaf7\" \
             stroke-width=\"2\"/>\
             <text x=\"{:.1}\" y=\"{:.1}\" font-family=\"ui-monospace, Menlo, monospace\" \
             font-size=\"15\" fill=\"#1c1a17\" stroke=\"#fbfaf7\" stroke-width=\"3.5\" \
             paint-order=\"stroke\">{}</text>",
            x + 10.0,
            y + 5.0,
            escape(&entity.name)
        ));
    }

    svg.push_str("</svg>");
    Some(format!(
        "<figure class=\"wide\">{svg}<figcaption>The world as it stood on {}. A border two \
         claims share, or one nobody dated, is hatched rather than assigned.</figcaption></figure>",
        escape(&world.calendar.format_long(day))
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_examples_everybody_checks_against() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn the_example_worlds_map_gives_up_its_dimensions_from_the_header_alone() {
        let bytes = std::fs::read("../../examples/vashen/map/vashen.png").expect("the map");
        let (w, h) = png_size(&bytes).expect("a PNG");
        assert!(w > 100 && h > 100, "{w}×{h}");
    }

    #[test]
    fn something_that_is_not_a_png_is_declined_rather_than_misread() {
        assert!(png_size(b"GIF89a not a png at all, but long enough to index into").is_none());
        assert!(png_size(b"tiny").is_none());
    }
}
