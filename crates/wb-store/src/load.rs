//! Reading a world off disk.
//!
//! Entities may be Markdown with YAML frontmatter (prose lives in the body) or plain
//! YAML (no prose). Both are the writer's files, in the writer's folders, editable
//! without this application.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::error::{Error, Result};
use crate::frontmatter;
use crate::model::{Entity, Event, Scene, WorldDef};
use crate::world::World;

const EXTENSIONS: [&str; 4] = ["md", "markdown", "yaml", "yml"];

/// Load the world rooted at `root`, which must contain `world.yaml`.
pub fn load(root: impl AsRef<Path>) -> Result<World> {
    let root = root.as_ref();
    let world_file = root.join("world.yaml");
    if !world_file.is_file() {
        return Err(Error::NoWorldFile { root: root.to_path_buf() });
    }

    let def: WorldDef = read_yaml(&world_file)?;

    let mut entities = Vec::new();
    for path in collect(&root.join("entities"))? {
        let (yaml, body) = read_document(&path)?;
        let mut entity: Entity = parse_yaml(&path, &yaml)?;
        entity.body = body;
        entity.source = path;
        entities.push(entity);
    }

    let mut events = Vec::new();
    for path in collect(&root.join("events"))? {
        let (yaml, body) = read_document(&path)?;
        let mut event: Event = parse_yaml(&path, &yaml)?;
        event.body = body;
        event.source = path;
        events.push(event);
    }

    // Scenes carry no prose of their own — the prose is the manuscript's, and the scene
    // only points at it — so the body a `.md` file would have is meaningless here and the
    // folder is plain YAML.
    let mut scenes = Vec::new();
    for path in collect(&root.join("scenes"))? {
        let yaml = read_text(&path)?;
        guard_source_key(&path, &yaml)?;
        let mut scene: Scene = parse_yaml(&path, &yaml)?;
        scene.source = path;
        scenes.push(scene);
    }

    World::assemble(root.to_path_buf(), def, entities, events, scenes)
}

/// Refuse a scene whose manuscript link is spelled `source:`.
///
/// Worth the extra parse because the failure it prevents is silent: `Scene::source` is
/// `#[serde(skip)]`, so a `source:` key is not a field, is not an error, and is simply
/// dropped — the writer would see their link in the file, the app would see no link at
/// all, and nothing anywhere would say why.
fn guard_source_key(path: &Path, yaml: &str) -> Result<()> {
    let doc: serde_yaml_bw::Value = serde_yaml_bw::from_str(yaml)
        .map_err(|e| Error::Yaml { path: path.to_path_buf(), message: e.to_string() })?;

    if let serde_yaml_bw::Value::Mapping(map) = doc
        && map.keys().any(|k| k.as_str() == Some("source"))
    {
        return Err(Error::SceneSourceKey { path: path.to_path_buf() });
    }
    Ok(())
}

/// Returns the YAML text and the prose body, which is empty for plain-YAML records.
fn read_document(path: &Path) -> Result<(String, String)> {
    let text = read_text(path)?;
    let is_markdown =
        path.extension().and_then(|e| e.to_str()).is_some_and(|e| e == "md" || e == "markdown");

    if !is_markdown {
        return Ok((text, String::new()));
    }

    let doc = frontmatter::split(&text)
        .ok_or_else(|| Error::MissingFrontmatter { path: path.to_path_buf() })?;
    Ok((doc.frontmatter.to_string(), doc.body.to_string()))
}

fn read_text(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| Error::Io { path: path.to_path_buf(), source })
}

fn parse_yaml<T: DeserializeOwned>(path: &Path, text: &str) -> Result<T> {
    serde_yaml_bw::from_str(text)
        .map_err(|e| Error::Yaml { path: path.to_path_buf(), message: e.to_string() })
}

fn read_yaml<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = read_text(path)?;
    parse_yaml(path, &text)
}

/// Every record file under `dir`, sorted so loading is deterministic.
fn collect(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk(dir, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    let io_err = |source| Error::Io { path: dir.to_path_buf(), source };

    for entry in fs::read_dir(dir).map_err(io_err)? {
        let path = entry.map_err(io_err)?.path();

        // Skips .git, .DS_Store, editor swapfiles, and the derived index.
        if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.')) {
            continue;
        }

        if path.is_dir() {
            walk(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e))
        {
            out.push(path);
        }
    }
    Ok(())
}
