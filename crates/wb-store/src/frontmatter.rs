//! Splitting `---`-delimited YAML frontmatter from a Markdown body.
//!
//! Same convention Obsidian and every static site generator uses, so a writer's
//! existing tooling can open these files without knowing anything about Worldbuilder.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Document<'a> {
    pub frontmatter: &'a str,
    pub body: &'a str,
}

/// `None` when the text does not open with a frontmatter block, or never closes it.
pub fn split(text: &str) -> Option<Document<'_>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let after_open = open_fence(text)?;

    let mut offset = 0;
    for line in after_open.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return Some(Document {
                frontmatter: &after_open[..offset],
                body: strip_one_newline(&after_open[offset + line.len()..]),
            });
        }
        offset += line.len();
    }
    None
}

/// Consumes a bare `---` line, rejecting `----` and a `---` not followed by a newline.
fn open_fence(s: &str) -> Option<&str> {
    let rest = s.strip_prefix("---")?;
    rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n'))
}

fn strip_one_newline(s: &str) -> &str {
    s.strip_prefix("\r\n").or_else(|| s.strip_prefix('\n')).unwrap_or(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_frontmatter_from_body() {
        let doc =
            split("---\nid: act_aldric\nname: Aldric\n---\nHe was born in the Vale.\n").unwrap();
        assert_eq!(doc.frontmatter, "id: act_aldric\nname: Aldric\n");
        assert_eq!(doc.body, "He was born in the Vale.\n");
    }

    #[test]
    fn handles_windows_line_endings() {
        let doc = split("---\r\nid: act_aldric\r\n---\r\nProse.\r\n").unwrap();
        assert_eq!(doc.frontmatter, "id: act_aldric\r\n");
        assert_eq!(doc.body, "Prose.\r\n");
    }

    #[test]
    fn tolerates_a_byte_order_mark() {
        assert!(split("\u{feff}---\nid: x\n---\nbody\n").is_some());
    }

    #[test]
    fn an_empty_body_is_fine() {
        let doc = split("---\nid: x\n---\n").unwrap();
        assert_eq!(doc.body, "");
    }

    #[test]
    fn horizontal_rules_inside_the_body_are_not_the_closing_fence() {
        let doc = split("---\nid: x\n---\nintro\n\n---\n\nmore prose\n").unwrap();
        assert_eq!(doc.frontmatter, "id: x\n");
        assert!(doc.body.starts_with("intro"), "body was {:?}", doc.body);
        assert!(doc.body.contains("more prose"));
    }

    #[test]
    fn rejects_documents_without_a_block() {
        assert_eq!(split("no frontmatter here\n"), None);
        assert_eq!(split("---\nid: x\nnever closed\n"), None);
        assert_eq!(split("----\nid: x\n----\n"), None, "a four-dash rule is not a fence");
        assert_eq!(split("---"), None);
    }
}
