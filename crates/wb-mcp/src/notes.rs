//! Reading the writer's raw notes.
//!
//! The single biggest barrier to adopting a tool like this is re-entering years of
//! existing notes by hand. So the world folder has a `notes/` drawer, and an agent can
//! read it — which is what makes `world-from-notes` work with nothing attached but this
//! server, no filesystem access required.
//!
//! **Scoped, and the scoping is the point.** This server is handed the writer's world
//! folder, not their disk. Every path is resolved against `notes/` and checked after
//! canonicalization, so a symlink pointing out of the folder is refused as firmly as
//! `../../.ssh/id_rsa` is.

use std::fs;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Serialize;
use wb_store::sandbox::Denied;

pub const DIR: &str = "notes";

/// Read whole only up to here; past it, `read_note` returns a head and says so. A
/// 400 KB manuscript dumped into a context window helps nobody.
const MAX_BYTES: usize = 120_000;

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct NoteDto {
    /// Pass this back to `read_note`.
    pub path: String,
    pub bytes: u64,
    /// First line of the file, which for Markdown is usually its title.
    pub first_line: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct NoteBody {
    pub path: String,
    pub text: String,
    pub bytes: u64,
    /// True when only the head of the file was returned.
    pub truncated: bool,
}

/// Every readable note, shallowest first.
pub fn list(root: &Path) -> Result<Vec<NoteDto>, String> {
    let dir = root.join(DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut found = Vec::new();
    walk(&dir, &mut found)?;
    found.sort();

    Ok(found
        .iter()
        .map(|path| {
            let bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            NoteDto {
                path: path.strip_prefix(root).unwrap_or(path).display().to_string(),
                bytes,
                first_line: head_line(path),
            }
        })
        .collect())
}

pub fn read(root: &Path, requested: &str) -> Result<NoteBody, String> {
    let path = resolve(root, requested)?;
    let text = fs::read_to_string(&path).map_err(|e| format!("cannot read `{requested}`: {e}"))?;
    let bytes = text.len() as u64;

    let truncated = text.len() > MAX_BYTES;
    let text = if truncated {
        let end = (0..=MAX_BYTES).rev().find(|&i| text.is_char_boundary(i)).unwrap_or(0);
        format!("{}\n\n[…truncated at {MAX_BYTES} bytes of {bytes}]", &text[..end])
    } else {
        text
    };

    Ok(NoteBody {
        path: path.strip_prefix(root).unwrap_or(&path).display().to_string(),
        text,
        bytes,
        truncated,
    })
}

/// Resolve a requested path inside `notes/`, or refuse.
///
/// The containment check itself lives in [`wb_store::sandbox`], which the manuscript
/// reader shares. What stays here is the phrasing: an agent that has just been refused
/// needs to be told about `list_notes`, and advice about `list_notes` would be useless
/// attached to a chapter of somebody's novel.
pub fn resolve(root: &Path, requested: &str) -> Result<PathBuf, String> {
    let notes = root.join(DIR);

    // Both spellings are accepted, since `list_notes` returns `notes/foo.md` and a model
    // reasonably shortens that to `foo.md`.
    let relative = Path::new(requested.trim());
    let stripped = relative.strip_prefix(DIR).unwrap_or(relative);

    wb_store::sandbox::resolve(&notes, &stripped.to_string_lossy()).map_err(|denied| match denied {
        Denied::Absolute => format!(
            "`{requested}` is an absolute path. Notes are addressed relative to the world \
             folder, as `list_notes` returns them."
        ),
        Denied::NoBase => format!("this world has no `{DIR}/` folder to read from"),
        Denied::Missing => {
            format!("no note at `{requested}`. Use `list_notes` to see what is there.")
        }
        Denied::Outside => format!(
            "`{requested}` resolves outside the world's `{DIR}/` folder. This server reads \
             the writer's notes, not their disk."
        ),
        Denied::NotAFile => format!("`{requested}` is a folder, not a note."),
    })
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read `{}`: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.')) {
            continue;
        }
        if path.is_dir() {
            walk(&path, out)?;
        } else if readable(&path) {
            out.push(path);
        }
    }
    Ok(())
}

/// Text formats only. A writer's notes folder collects PDFs and images too, and
/// offering to read one as UTF-8 only produces a confusing failure later.
fn readable(path: &Path) -> bool {
    const EXTENSIONS: [&str; 6] = ["md", "markdown", "txt", "text", "org", "rst"];
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

fn head_line(path: &Path) -> String {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| {
            text.lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(|line| line.trim_start_matches('#').trim().chars().take(120).collect())
        })
        .unwrap_or_default()
}
