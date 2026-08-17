//! Reading the book, and only reading it.
//!
//! Everything in this module is one-directional by construction: there is no function
//! here that writes, and adding one would be the first step toward owning prose that
//! belongs to Scrivener, Obsidian, or Word. DESIGN.md's decision #7 says the app links
//! the manuscript and never edits it, and the cheapest way to keep a promise like that
//! is to have no code that could break it.
//!
//! A link is `ch12.md#the-breach`: a file relative to the declared manuscript root, and
//! optionally the heading of one section inside it. Both halves are resolved late, on
//! read, so a record keeps the string the writer typed and travels in git unharmed.

use std::path::{Path, PathBuf};

use wb_store::World;
use wb_store::sandbox::{self, Denied};

/// Read whole only up to here. The same ceiling `read_note` uses, for the same reason:
/// a 400 KB chapter dumped into a context window helps nobody.
const MAX_BYTES: usize = 120_000;

/// Text formats only. A manuscript folder collects `.docx`, `.scriv` bundles and PDFs,
/// and offering to read one as UTF-8 produces a confusing failure two layers later.
const EXTENSIONS: [&str; 6] = ["md", "markdown", "txt", "text", "org", "rst"];

/// A slice of the manuscript, and where it came from.
#[derive(Debug, Clone)]
pub struct Passage {
    /// As the scene spelled it, relative to the manuscript root.
    pub file: String,
    /// The fragment asked for, if there was one.
    pub anchor: Option<String>,
    /// The heading actually matched, in the words the writer wrote it in.
    pub heading: Option<String>,
    pub text: String,
    pub words: usize,
    pub truncated: bool,
}

/// The manuscript root as an absolute path, or `None` if this world declares no book.
///
/// Not canonicalized here — [`sandbox::resolve`] does that, and doing it twice would
/// mean two different answers when the folder is missing.
pub fn root(world: &World) -> Option<PathBuf> {
    world.manuscript.as_ref().map(|m| world.root.join(&m.root))
}

/// Every readable chapter under the manuscript root, in reading order.
///
/// Declared order first, then everything else lexically. A writer who numbers their
/// chapters gets the right answer for free; one who reaches `ch10` without zero-padding
/// lists them in `world.yaml` and gets it right anyway.
pub fn chapters(base: &Path, declared: &[String]) -> Vec<String> {
    let mut found = Vec::new();
    walk(base, base, &mut found);
    found.sort();

    let mut out: Vec<String> = Vec::new();
    for name in declared {
        if let Some(i) = found.iter().position(|f| f == name) {
            out.push(found.remove(i));
        }
    }
    out.extend(found);
    out
}

/// Read what a scene's `prose:` link points at.
pub fn read(base: &Path, link: &str) -> Result<Passage, String> {
    let (file, anchor) = split(link);

    let path = sandbox::resolve(base, file).map_err(|denied| match denied {
        Denied::Absolute => format!(
            "`{link}` is an absolute path. A scene's prose link is relative to the \
             manuscript root declared in world.yaml."
        ),
        Denied::NoBase => {
            "the manuscript root in world.yaml does not point at a folder that exists".into()
        }
        Denied::Missing => format!("no chapter at `{file}` under the manuscript root"),
        Denied::Outside => format!(
            "`{link}` resolves outside the manuscript root. The app reads the book you \
             pointed it at, not the rest of your disk."
        ),
        Denied::NotAFile => format!("`{file}` is a folder, not a chapter"),
    })?;

    let whole = std::fs::read_to_string(&path).map_err(|e| format!("cannot read `{file}`: {e}"))?;

    let (text, heading) = match anchor {
        None => (whole, None),
        Some(want) => {
            let (slice, heading) = section(&whole, want).ok_or_else(|| {
                format!("`{file}` has no heading matching `#{want}`. The link still points somewhere real, but not at a section.")
            })?;
            (slice.to_string(), Some(heading.to_string()))
        }
    };

    let truncated = text.len() > MAX_BYTES;
    let text = if truncated {
        let end = (0..=MAX_BYTES).rev().find(|&i| text.is_char_boundary(i)).unwrap_or(0);
        text[..end].to_string()
    } else {
        text
    };

    Ok(Passage {
        file: file.to_string(),
        anchor: anchor.map(str::to_string),
        heading,
        words: text.split_whitespace().count(),
        text,
        truncated,
    })
}

/// `ch12.md#the-breach` into its two halves.
fn split(link: &str) -> (&str, Option<&str>) {
    match link.trim().split_once('#') {
        Some((file, anchor)) if !anchor.is_empty() => (file, Some(anchor)),
        Some((file, _)) => (file, None),
        None => (link.trim(), None),
    }
}

/// The section under the heading whose slug is `want`, and the heading's own text.
///
/// Runs to the next heading of the same or higher level, so `## The breach` ends at the
/// next `##` or `#` and *contains* any `###` beneath it. Ending at the next heading of
/// any level instead would silently truncate a scene at its first sub-break, which is a
/// wrong answer that looks like a right one.
fn section<'a>(text: &'a str, want: &str) -> Option<(&'a str, &'a str)> {
    let mut start: Option<(usize, usize, &str)> = None; // byte offset, level, heading text

    for (offset, line) in line_offsets(text) {
        let Some((level, title)) = heading(line) else { continue };

        match start {
            None if slug(title) == want => start = Some((offset, level, title)),
            Some((from, opened, title)) if level <= opened => {
                return Some((&text[from..offset], title));
            }
            _ => {}
        }
    }

    start.map(|(from, _, title)| (&text[from..], title))
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.bytes().take_while(|b| *b == b'#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    // `#Not a heading` is not one; ATX headings need the space.
    if !rest.starts_with(' ') {
        return None;
    }
    Some((hashes, rest.trim()))
}

/// GitHub's heading slug, which is the spelling every Markdown editor's "copy link to
/// heading" produces, and therefore the one a writer will paste in.
fn slug(title: &str) -> String {
    let mut out = String::new();
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
        } else if ch == ' ' || ch == '-' || ch == '_' {
            out.push('-');
        }
    }
    out
}

fn line_offsets(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut at = 0;
    text.split_inclusive('\n').map(move |line| {
        let start = at;
        at += line.len();
        (start, line.trim_end_matches(['\n', '\r']))
    })
}

fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.')) {
            continue;
        }
        if path.is_dir() {
            walk(base, &path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
            && let Ok(relative) = path.strip_prefix(base)
        {
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHAPTER: &str = "\
# Twelve — The Siege

Front matter nobody linked to.

## The breach

They came when the passes were dry.

### A beat inside the scene

This still belongs to the breach.

## After

This does not.
";

    #[test]
    fn an_anchor_takes_the_section_and_stops_at_the_next_heading_of_its_level() {
        let (text, heading) = section(CHAPTER, "the-breach").expect("found");
        assert!(text.contains("passes were dry"));
        assert!(text.contains("still belongs"), "a deeper heading is part of the section");
        assert!(!text.contains("This does not"), "and a sibling heading ends it");
        assert_eq!(heading, "The breach");
    }

    #[test]
    fn the_last_section_runs_to_the_end_of_the_file() {
        let (text, _) = section(CHAPTER, "after").expect("found");
        assert!(text.trim_end().ends_with("This does not."));
    }

    #[test]
    fn an_anchor_nothing_matches_is_not_silently_the_whole_file() {
        assert!(section(CHAPTER, "the-retreat").is_none());
    }

    #[test]
    fn a_link_splits_into_a_file_and_an_optional_anchor() {
        assert_eq!(split("ch12.md#the-breach"), ("ch12.md", Some("the-breach")));
        assert_eq!(split("ch12.md"), ("ch12.md", None));
        assert_eq!(split("  ch12.md  "), ("ch12.md", None));
        assert_eq!(split("ch12.md#"), ("ch12.md", None), "an empty fragment is no fragment");
    }

    #[test]
    fn headings_slug_the_way_a_markdown_editor_spells_them() {
        assert_eq!(slug("The gate at dusk"), "the-gate-at-dusk");
        assert_eq!(slug("Twelve — The Siege"), "twelve--the-siege");
        assert_eq!(slug("Chapter 1: Marrow"), "chapter-1-marrow");
    }

    #[test]
    fn a_hash_without_a_space_is_not_a_heading() {
        assert_eq!(heading("#not-a-heading"), None);
        assert_eq!(heading("## Yes"), Some((2, "Yes")));
        assert_eq!(heading("####### too deep"), None);
    }

    /// Declared order wins, and anything the writer forgot to declare still shows up.
    #[test]
    fn chapters_take_the_declared_order_first_and_the_rest_lexically() {
        let dir = std::env::temp_dir().join(format!("wb-chapters-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["ch1.md", "ch2.md", "ch10.md", "notes.png"] {
            std::fs::write(dir.join(name), "# x\n").unwrap();
        }

        let declared = vec!["ch1.md".to_string(), "ch2.md".to_string(), "ch10.md".to_string()];
        assert_eq!(chapters(&dir, &declared), declared);
        assert_eq!(
            chapters(&dir, &[]),
            vec!["ch1.md", "ch10.md", "ch2.md"],
            "lexical order is what un-padded numbering deserves, and why `order` exists"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
