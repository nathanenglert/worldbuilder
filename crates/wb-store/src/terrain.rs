//! Terrain, and the cache that keeps it from being rebuilt.
//!
//! Terrain is the one thing in a world that is *derived*. Everything else in `wb-store`
//! is the writer's own text, read and never rewritten; terrain is computed from a map
//! image and a few dozen numbers, takes about a second, and is identical every time.
//!
//! So it goes in `.worldbuilder/`, which is gitignored — a build product, not canon.
//! Deleting it costs a rebuild and nothing else. The key is the parameters' digest mixed
//! with the image's own bytes, which means editing the map or nudging a slider produces
//! a different file rather than a stale hit.

use std::path::{Path, PathBuf};

use wb_terrain::Terrain;
use wb_terrain::rng::Digest;

use crate::error::{Error, Result};
use crate::model::MapSpec;
use crate::world::World;

const CACHE_DIR: &str = ".worldbuilder";

impl World {
    /// The terrain under this world.
    ///
    /// `None` when `world.yaml` declares no `map:` — a world with no map image is a
    /// perfectly good world, and every caller has to cope with that anyway.
    ///
    /// Reads the cache when the digest matches, and otherwise rebuilds and writes it.
    /// A cache that cannot be read or written is not an error: the terrain is still
    /// correct, it just costs a second.
    pub fn terrain(&self) -> Result<Option<Terrain>> {
        let Some(spec) = &self.map else { return Ok(None) };
        build(&self.root, spec).map(Some)
    }
}

/// Where a world's terrain would be cached, given its key. Public so a caller that wants
/// to clear the cache does not have to guess the layout.
pub fn cache_path(root: &Path, key: u64) -> PathBuf {
    root.join(CACHE_DIR).join(format!("terrain-{key:016x}.json"))
}

fn build(root: &Path, spec: &MapSpec) -> Result<Terrain> {
    let image_path = root.join(&spec.image);
    let bytes = std::fs::read(&image_path)
        .map_err(|e| Error::MapImage { path: image_path.clone(), source: e })?;

    let key = {
        let mut d = Digest::new();
        d.u64(spec.terrain.digest()).bytes(&bytes);
        d.finish()
    };
    let path = cache_path(root, key);

    if let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(cached) = serde_json::from_str::<Terrain>(&text)
        && cached.digest == spec.terrain.digest()
    {
        return Ok(cached);
    }

    let image = wb_terrain::load_image(&bytes).map_err(|e| Error::Terrain(e.to_string()))?;
    let terrain =
        wb_terrain::build(&image, &spec.terrain).map_err(|e| Error::Terrain(e.to_string()))?;

    write_cache(&path, &terrain);
    Ok(terrain)
}

/// Best effort. A read-only world folder is a legitimate thing to open.
fn write_cache(path: &Path, terrain: &Terrain) {
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Ok(text) = serde_json::to_string(terrain) else { return };
    if std::fs::write(path, text).is_err() {
        return;
    }

    // Exactly one terrain is ever current, so the previous one is dead weight. Sweeping
    // here rather than on load keeps a world folder from quietly growing a megabyte per
    // twitch of the detail slider.
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let stale = entry.path();
        let is_terrain = stale
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("terrain-") && n.ends_with(".json"));
        if is_terrain && stale != path {
            let _ = std::fs::remove_file(stale);
        }
    }
}
