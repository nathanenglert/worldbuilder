//! The history, as one strip of SVG.
//!
//! The doubt bars are the part worth carrying over. A world whose events are drawn as
//! points has quietly told the reader that every date in it is known, which is the
//! opposite of what this tool is for — so an event dated `0812-04` gets a month-wide bar
//! under its marker, and one dated `0811~` gets a bar two years wide.

use std::collections::BTreeSet;

use wb_core::Day;
use wb_store::World;

use crate::html::{ANCHOR, escape};

const W: f64 = 1000.0;
const H: f64 = 132.0;
const AXIS: f64 = 92.0;

/// Every event in scope, along one axis, or `None` if fewer than two of them are dated.
pub fn figure(world: &World, included: &BTreeSet<String>) -> Option<String> {
    let mut dated: Vec<(&str, &str, i64, i64, i64)> = world
        .events
        .values()
        .filter(|e| included.contains(&e.id))
        .filter_map(|event| {
            let r = world.resolved_node(&event.id)?;
            let nominal = r.nominal?.0;
            Some((
                event.id.as_str(),
                event.name.as_str(),
                nominal,
                r.earliest.map_or(nominal, |d| d.0),
                r.latest.map_or(nominal, |d| d.0),
            ))
        })
        .collect();
    if dated.len() < 2 {
        return None;
    }
    dated.sort_by_key(|e| e.2);

    let lo = dated.iter().map(|e| e.3).min()?;
    let hi = dated.iter().map(|e| e.4).max()?;
    let pad = ((hi - lo) / 14).max(1);
    let (lo, hi) = (lo - pad, hi + pad);
    let width = (hi - lo).max(1) as f64;
    let x = |day: i64| ((day - lo) as f64 / width) * W;

    let mut svg = format!(
        "<svg viewBox=\"0 0 {W:.0} {H:.0}\" role=\"img\" aria-label=\"The history\" \
         xmlns=\"http://www.w3.org/2000/svg\">\
         <line x1=\"0\" y1=\"{AXIS}\" x2=\"{W:.0}\" y2=\"{AXIS}\" stroke=\"#7d766b\" \
         stroke-opacity=\".5\" stroke-width=\"1\"/>"
    );

    // Labels alternate between two heights so two close events do not overprint.
    for (i, (id, name, nominal, earliest, latest)) in dated.iter().enumerate() {
        let cx = x(*nominal);
        // Centred text at either end of the strip would run past the viewBox and be
        // clipped, which is how an event silently loses its name.
        let anchor = if cx > W * 0.86 {
            "end"
        } else if cx < W * 0.14 {
            "start"
        } else {
            "middle"
        };
        if latest > earliest {
            svg.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"7\" fill=\"#7a5c2e\" \
                 fill-opacity=\".28\"/>",
                x(*earliest),
                AXIS - 3.5,
                (x(*latest) - x(*earliest)).max(1.5)
            ));
        }
        svg.push_str(&format!(
            "<path d=\"M {cx:.1} {} l 6 6 l -6 6 l -6 -6 Z\" fill=\"#7a5c2e\"/>",
            AXIS - 6.0
        ));
        let y = if i % 2 == 0 { AXIS - 20.0 } else { AXIS - 44.0 };
        svg.push_str(&format!(
            "<line x1=\"{cx:.1}\" y1=\"{:.1}\" x2=\"{cx:.1}\" y2=\"{:.1}\" stroke=\"#7d766b\" \
             stroke-opacity=\".45\"/>\
             <a href=\"#{ANCHOR}{}\"><text x=\"{cx:.1}\" y=\"{y:.1}\" text-anchor=\"{anchor}\" \
             font-family=\"ui-monospace, Menlo, monospace\" font-size=\"13\" \
             fill=\"#1c1a17\">{}</text></a>",
            AXIS - 10.0,
            y + 4.0,
            escape(id),
            escape(name)
        ));

        let when = world
            .events
            .get(*id)
            .and_then(|e| wb_store::phrasing::phrase(world, id, &e.date))
            .unwrap_or_else(|| world.calendar.format_numeric(Day(*nominal)));
        svg.push_str(&format!(
            "<text x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"{anchor}\" \
             font-family=\"ui-monospace, Menlo, monospace\" font-size=\"12\" \
             fill=\"#7d766b\">{}</text>",
            AXIS + 20.0,
            escape(&when)
        ));
    }

    svg.push_str("</svg>");
    Some(format!(
        "<figure class=\"wide\">{svg}<figcaption>Every dated event in this document. The bar \
         under a marker is how wide the doubt is, not how long the event took.</figcaption>\
         </figure>"
    ))
}
