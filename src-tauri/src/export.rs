//! Publishing, which is a thing a person does.
//!
//! There is no MCP tool behind any of this and there is not going to be. `protocol.rs`
//! asserts the writing tools are *exactly* `["propose_changes"]`; an agent that could
//! write a file anywhere on the disk under the name of "export" would make that assertion
//! true and meaningless at the same time.
//!
//! Two guards on the write itself, both of them because this is the first place in the
//! app that puts a file somewhere the writer chose rather than somewhere the world owns:
//! it must be an `.html` path, and it never silently replaces something already there.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;
use wb_core::Day;
use wb_export::Scope;
use wb_store::World;

use crate::commands::AppState;

#[derive(Serialize)]
pub struct ExportPreviewDto {
    /// What the scope came out as, in words: `as it stood on 12 Verdant, 812 AR`.
    pub caption: String,
    pub bytes: usize,
    pub records: usize,
    /// Records in the world that this scope leaves out. Stated, so a small number is
    /// visibly a choice and not a failure.
    pub omitted: usize,
    pub links: usize,
    /// Where it would go if the writer does not say otherwise: beside the world folder,
    /// never inside it, so a published document never becomes part of canon.
    pub suggested: String,
}

#[derive(Serialize)]
pub struct ExportWroteDto {
    pub path: String,
    pub bytes: usize,
}

/// `everything` · `as-of` · `on-the-page`, with the date only the middle one uses.
fn scope_of(world: &World, scope: &str, at: Option<String>) -> Result<Scope, String> {
    match scope {
        "everything" => Ok(Scope::Everything),
        "on-the-page" => Ok(Scope::OnThePage),
        "as-of" => {
            let expr = at.unwrap_or_default();
            let expr = expr.trim();
            if expr.is_empty() {
                return Err("`as it stood` needs a date to stand on.".into());
            }
            match world.day_of(expr) {
                Ok(Some(day)) => Ok(Scope::AsOf(Day(day.0))),
                Ok(None) => Err(format!("`{expr}` has no position on this world's timeline.")),
                Err(e) => Err(format!("`{expr}` is not a date this world understands: {e}")),
            }
        }
        other => Err(format!("`{other}` is not a scope")),
    }
}

fn caption(world: &World, scope: Scope) -> String {
    match scope {
        Scope::Everything => "everything in this world".to_string(),
        Scope::AsOf(day) => format!("as it stood on {}", world.calendar.format_long(day)),
        Scope::OnThePage => "only what the book names".to_string(),
    }
}

fn file_slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() { "world".to_string() } else { trimmed }
}

fn suggested(world: &World) -> PathBuf {
    let beside = world.root.parent().unwrap_or(&world.root);
    beside.join(format!("{}.html", file_slug(&world.name)))
}

#[tauri::command]
pub fn preview_export(
    scope: String,
    at: Option<String>,
    state: State<'_, AppState>,
) -> Result<ExportPreviewDto, String> {
    state.read(|world| {
        let scope = scope_of(world, &scope, at)?;
        let document = wb_export::bible(world, scope);
        let records = document.matches("<article id=").count();

        Ok(ExportPreviewDto {
            caption: caption(world, scope),
            bytes: document.len(),
            records,
            omitted: (world.entities.len() + world.events.len()).saturating_sub(records),
            links: document.matches("href=\"#").count(),
            suggested: suggested(world).display().to_string(),
        })
    })?
}

#[tauri::command]
pub fn write_export(
    scope: String,
    at: Option<String>,
    path: String,
    overwrite: bool,
    state: State<'_, AppState>,
) -> Result<ExportWroteDto, String> {
    state.read(|world| {
        let scope = scope_of(world, &scope, at)?;

        let target = PathBuf::from(path.trim());
        if target.as_os_str().is_empty() {
            return Err("where should it go?".into());
        }
        if !matches!(
            target.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(),
            Some("html" | "htm")
        ) {
            return Err(format!(
                "`{}` is not an .html file. This writes one page, not a folder.",
                target.display()
            ));
        }
        if target.is_dir() {
            return Err(format!("`{}` is a folder.", target.display()));
        }
        if target.exists() && !overwrite {
            return Err(format!(
                "`{}` is already there. Press again to replace it.",
                target.display()
            ));
        }
        let parent = target.parent().unwrap_or(Path::new("."));
        if !parent.as_os_str().is_empty() && !parent.is_dir() {
            return Err(format!("there is no folder `{}`.", parent.display()));
        }

        let document = wb_export::bible(world, scope);
        std::fs::write(&target, &document)
            .map_err(|e| format!("could not write {}: {e}", target.display()))?;

        Ok(ExportWroteDto { path: target.display().to_string(), bytes: document.len() })
    })?
}
