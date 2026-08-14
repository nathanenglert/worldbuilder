//! Draws the map image that ships with `examples/vashen`.
//!
//! Stage 1 of the pipeline is "import raster" — a Wonderdraft export, a commission, a
//! phone photo of a napkin. The example world has no commissioned art, so this stands in
//! for it, and it is generated rather than drawn so the repo carries the recipe instead
//! of an opaque blob. Nothing else in Worldbuilder generates a map; this is a prop.
//!
//! The geography is not arbitrary. It is read off the world that already exists:
//!
//! - The notes say the Silt "runs west out of the Rimefall hills, past Greyford, past
//!   Marrow, into the Vale", so the sea is west and the high ground is east.
//! - The Vale of Corrath reaches `[0.14, 0.47]`, so the firth reaches back to meet it.
//!   Corrath at `[0.25, 0.45]` is then a market and a garrison a short ride from the head
//!   of navigation, which is a city with a reason to be where it is.
//! - Marrow at `[0.43, 0.40]` is "the last thing standing between Vashen and the Vale",
//!   so the range between the two is drawn — in `world.yaml`, not here — in two pieces
//!   with a gap at Marrow's latitude. The wall town holds a pass, and the river uses it.
//!
//! Run with `cargo run -p wb-terrain --example vashen_map`.

use std::path::PathBuf;

use image::{ImageEncoder, Rgba, RgbaImage, codecs::png::PngEncoder};
use wb_terrain::rng::Rng;

const W: u32 = 2000;
const H: u32 = 1400;
const ASPECT: f64 = W as f64 / H as f64;

const SEA: [u8; 3] = [0x1E, 0x3A, 0x4C];
const LAND: [u8; 3] = [0xC9, 0xBF, 0xA6];

/// Centre and radius, in normalized units. The grain field distorts them past
/// recognition, which is the point — a circle in the sea reads as generated.
const ISLANDS: [([f64; 2], f64); 6] = [
    ([0.046, 0.271], 0.033),
    ([0.030, 0.352], 0.014),
    ([0.061, 0.618], 0.021),
    ([0.038, 0.690], 0.010),
    ([0.213, 0.876], 0.026),
    ([0.402, 0.913], 0.015),
];

fn main() {
    let img = draw();

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/vashen/map");
    std::fs::create_dir_all(&out).expect("create map directory");
    let path = out.join("vashen.png");

    let file = std::fs::File::create(&path).expect("create map image");
    PngEncoder::new(std::io::BufWriter::new(file))
        .write_image(img.as_raw(), W, H, image::ExtendedColorType::Rgba8)
        .expect("encode map image");

    println!("{} — {W}×{H}, {:.1}% land", path.display(), land_fraction(&img) * 100.0);
}

/// The noise fields the coastline is made of.
struct Fields {
    /// Where the western shore sits, as a function of latitude.
    coast: Fbm,
    /// Where the southern shore sits, as a function of longitude.
    south: Fbm,
    /// Fine displacement at every scale. This is what the detail slider removes.
    grain: Fbm,
    /// Two fields that displace where the others are sampled.
    warp_x: Fbm,
    warp_y: Fbm,
    /// Paper tone. Purely cosmetic, and well inside the segmentation tolerance.
    paper: Fbm,
}

fn draw() -> RgbaImage {
    let n = Fields {
        coast: Fbm::new(0x5117, 3, 5, 0.55),
        south: Fbm::new(0x5007, 3, 5, 0.55),
        grain: Fbm::new(0xC0A57, 9, 7, 0.58),
        warp_x: Fbm::new(0x3A11, 4, 3, 0.5),
        warp_y: Fbm::new(0x3A12, 4, 3, 0.5),
        paper: Fbm::new(0xBA5E, 20, 3, 0.5),
    };

    let mut img = RgbaImage::new(W, H);
    for y in 0..H {
        for x in 0..W {
            let p = [f64::from(x) / f64::from(W), f64::from(y) / f64::from(H)];

            // Supersample the coast so it antialiases the way a real export does — which
            // is exactly the mess `tolerance` and `min_blob_px` exist to cope with.
            let hits = [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)]
                .iter()
                .filter(|(dx, dy)| {
                    field([p[0] + dx / f64::from(W), p[1] + dy / f64::from(H)], &n) > 0.0
                })
                .count();

            let base = mix(SEA, LAND, hits as f64 / 4.0);
            let tone = n.paper.at(p[0], p[1]) * 7.0;
            let px = base.map(|c| (f64::from(c) + tone).clamp(0.0, 255.0) as u8);
            img.put_pixel(x, y, Rgba([px[0], px[1], px[2], 255]));
        }
    }

    dust(&mut img);
    img
}

/// Wrong-colour specks, the residue of every real export: JPEG ringing, a stray brush,
/// antialiased lettering. Stage 2 has to eat these, or the world acquires three hundred
/// one-pixel islands and a coastline made of confetti.
fn dust(img: &mut RgbaImage) {
    let mut rng = Rng::new(0x5EC4);
    for _ in 0..1400 {
        let x = rng.below(W as usize) as i64;
        let y = rng.below(H as usize) as i64;
        let flip = if rng.f64() < 0.5 { SEA } else { LAND };
        // A quarter of them are small blobs rather than single pixels, so despeckling has
        // graded work rather than one trivial size.
        let r = if rng.f64() < 0.25 { 1 } else { 0 };
        for dy in -r..=r {
            for dx in -r..=r {
                let (px, py) = (x + dx, y + dy);
                if px >= 0 && py >= 0 && px < i64::from(W) && py < i64::from(H) {
                    img.put_pixel(px as u32, py as u32, Rgba([flip[0], flip[1], flip[2], 255]));
                }
            }
        }
    }
}

/// Positive is land. Normalized coordinates throughout, `y` growing southward.
fn field(p: [f64; 2], n: &Fields) -> f64 {
    let [x, y] = p;

    // Domain warp: sample the other fields at a point that is itself displaced. This is
    // what turns a wobbling line into headlands and inlets — without it the coast reads
    // as a sine wave no matter how many octaves are stacked on it.
    let wx = x + 0.075 * n.warp_x.at(x, y);
    let wy = y + 0.075 * n.warp_y.at(x, y);

    // The western shore. Its amplitude varies with latitude: broken and peninsular to the
    // north and south, quiet where the Vale comes down to the water, because a firth full
    // of skerries is not somewhere a duchy runs its grain through.
    let calm = gauss(y, 0.47, 0.105);
    let reach = 0.026 + 0.115 * (1.0 - calm);
    let west = x - (0.115 + reach * (0.5 + 0.5 * n.coast.at(0.19, wy)));

    // The southern shore. The map is a corner of a continent, not the whole of one, so
    // north and east simply run off the edge.
    let southern = (0.735 + 0.062 * n.south.at(wx, 0.73)) - y;

    // The firth: the Vale's way to the sea, tapering inland to the head of navigation.
    let (d, t) = seg_distance(p, [0.070, 0.524], [0.190, 0.500]);
    let firth = d - (0.058 * (1.0 - t) + 0.010 * t);

    let mut f = west.min(southern).min(firth);
    for (c, r) in ISLANDS {
        f = f.max(r - hypot_aspect(p, c));
    }

    f + 0.052 * n.grain.at(wx, wy)
}

/// A bump of height 1 at `mu`, falling off over `sigma`.
fn gauss(v: f64, mu: f64, sigma: f64) -> f64 {
    (-((v - mu) / sigma).powi(2)).exp()
}

/// Distance from `p` to the segment `a..b`, and how far along it the nearest point lies.
fn seg_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> (f64, f64) {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / len2).clamp(0.0, 1.0)
    };
    (hypot_aspect(p, [a[0] + ab[0] * t, a[1] + ab[1] * t]), t)
}

fn hypot_aspect(a: [f64; 2], b: [f64; 2]) -> f64 {
    f64::hypot((a[0] - b[0]) * ASPECT, a[1] - b[1])
}

fn mix(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    std::array::from_fn(|i| (f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * t) as u8)
}

fn land_fraction(img: &RgbaImage) -> f64 {
    let nearer_land = |p: &Rgba<u8>| {
        let d2 = |c: [u8; 3]| {
            (0..3).map(|i| f64::from(p[i]) - f64::from(c[i])).map(|d| d * d).sum::<f64>()
        };
        d2(LAND) < d2(SEA)
    };
    img.pixels().filter(|p| nearer_land(p)).count() as f64 / (f64::from(W) * f64::from(H))
}

/// Value-noise fractal Brownian motion over a wrapping lattice.
struct Fbm {
    octaves: Vec<Lattice>,
    gain: f64,
}

impl Fbm {
    /// `base` is the coarsest lattice; each octave doubles it. `gain` is how fast the
    /// amplitude falls — 0.5 is smooth, 0.6 is craggy.
    fn new(seed: u64, base: usize, octaves: usize, gain: f64) -> Self {
        Self {
            octaves: (0..octaves)
                .map(|o| Lattice::new(base << o, seed.wrapping_add(o as u64 * 0x9E37)))
                .collect(),
            gain,
        }
    }

    /// Takes a point in `0..1` and returns roughly `-1..1`. Each octave's frequency is
    /// its lattice size, so the coordinate is never scaled — scaling both is the classic
    /// way to end up with an fBm whose octaves all sit on top of each other.
    fn at(&self, u: f64, v: f64) -> f64 {
        let (mut sum, mut amp, mut norm) = (0.0, 1.0, 0.0);
        for lat in &self.octaves {
            sum += amp * lat.at(u, v);
            norm += amp;
            amp *= self.gain;
        }
        (sum / norm) * 2.0 - 1.0
    }
}

struct Lattice {
    n: usize,
    v: Vec<f64>,
}

impl Lattice {
    fn new(n: usize, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self { n, v: (0..n * n).map(|_| rng.f64()).collect() }
    }

    /// Bilinear with a smoothstep, so there are no lattice creases. The domain wraps.
    fn at(&self, u: f64, v: f64) -> f64 {
        let n = self.n as f64;
        let (fx, fy) = ((u * n).rem_euclid(n), (v * n).rem_euclid(n));
        let (x0, y0) = (fx.floor() as usize % self.n, fy.floor() as usize % self.n);
        let (x1, y1) = ((x0 + 1) % self.n, (y0 + 1) % self.n);
        let (tx, ty) = (smooth(fx.fract()), smooth(fy.fract()));

        let g = |a: usize, b: usize| self.v[b * self.n + a];
        let top = g(x0, y0) + (g(x1, y0) - g(x0, y0)) * tx;
        let bot = g(x0, y1) + (g(x1, y1) - g(x0, y1)) * tx;
        top + (bot - top) * ty
    }
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}
