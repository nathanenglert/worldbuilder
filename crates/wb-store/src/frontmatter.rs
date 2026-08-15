//! Splitting `---`-delimited YAML frontmatter from a Markdown body.
//!
//! Same convention Obsidian and every static site generator uses, so a writer's
//! existing tooling can open these files without knowing anything about Worldbuilder.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Document<'a> {
    pub frontmatter: &'a str,
    pub body: &'a str,
}

/// The same split, as byte ranges into the original text.
///
/// This is what lets the writer rewrite a record without touching anything it did not
/// mean to. Everything outside `frontmatter` — the byte order mark, both fences, the
/// blank line after the closing one, every CRLF, and the whole prose body — is preserved
/// *structurally*, because the patcher never addresses a byte outside that range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spans {
    pub frontmatter: Range<usize>,
    pub body: Range<usize>,
}

/// `None` when the text does not open with a frontmatter block, or never closes it.
pub fn split_spans(text: &str) -> Option<Spans> {
    // Offsets are into `text` as given, so a BOM counts. `split` strips it and then
    // rebinds, which is right for slices and off by three for spans.
    let bom = if text.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    let rest = &text[bom..];
    let after_open = open_fence(rest)?;
    let start = bom + (rest.len() - after_open.len());

    let mut offset = 0;
    for line in after_open.split_inclusive('\n') {
        if line.trim_end() == "---" {
            let end = start + offset;
            let after_close = end + line.len();
            let body = &text[after_close..];
            let skipped = body.len() - strip_one_newline(body).len();
            return Some(Spans {
                frontmatter: start..end,
                body: (after_close + skipped)..text.len(),
            });
        }
        offset += line.len();
    }
    None
}

/// `None` when the text does not open with a frontmatter block, or never closes it.
pub fn split(text: &str) -> Option<Document<'_>> {
    let spans = split_spans(text)?;
    Some(Document {
        frontmatter: &text[spans.frontmatter.start..spans.frontmatter.end],
        body: &text[spans.body.start..spans.body.end],
    })
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

    /// The property the writer depends on: the two spans, plus everything between and
    /// around them, reconstruct the file exactly. Anything the patcher leaves alone is
    /// therefore byte-identical by construction rather than by care.
    #[test]
    fn the_spans_address_the_original_text_including_any_bom() {
        for text in [
            "---\nid: x\n---\nbody\n",
            "\u{feff}---\nid: x\n---\nbody\n",
            "---\r\nid: x\r\n---\r\nbody\r\n",
            "---\nid: x\n---\n",
        ] {
            let spans = split_spans(text).expect("splits");
            let doc = split(text).expect("splits");
            assert_eq!(&text[spans.frontmatter.start..spans.frontmatter.end], doc.frontmatter);
            assert_eq!(&text[spans.body.start..spans.body.end], doc.body);
            assert!(
                spans.frontmatter.end <= spans.body.start,
                "the body cannot start before the frontmatter ends"
            );
            assert_eq!(spans.body.end, text.len(), "the body runs to the end of the file");
        }
    }

    #[test]
    fn a_bom_is_outside_the_frontmatter_span() {
        let text = "\u{feff}---\nid: x\n---\nbody\n";
        let spans = split_spans(text).expect("splits");
        assert_eq!(spans.frontmatter.start, 7, "three bytes of BOM, then the open fence");
        assert_eq!(&text[..spans.frontmatter.start], "\u{feff}---\n");
    }

    #[test]
    fn rejects_documents_without_a_block() {
        assert_eq!(split("no frontmatter here\n"), None);
        assert_eq!(split("---\nid: x\nnever closed\n"), None);
        assert_eq!(split("----\nid: x\n----\n"), None, "a four-dash rule is not a fence");
        assert_eq!(split("---"), None);
    }
}
