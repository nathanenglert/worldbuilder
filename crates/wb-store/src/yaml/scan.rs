//! Where each key's bytes are, so a rewrite can leave the rest alone.
//!
//! This is not a YAML parser and must never be mistaken for one. It answers exactly one
//! question — *which bytes belong to which key* — and it is allowed to give up. Every
//! caller cross-checks it against libyaml, which is the authority on **what** is in the
//! file; the scanner only has to agree on **where**. When the two disagree the writer
//! falls back to a canonical rewrite, so the cost of the scanner being wrong is a wider
//! diff, never a corrupted file.
//!
//! That division is what makes a hand-rolled line walker defensible here. The classic
//! way this kind of code fails — a line at column zero inside a block scalar that looks
//! like a key — surfaces as an extra key the oracle does not have, and bails.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Block,
    Flow,
}

/// One `key: value` of a block mapping, located in the original bytes.
///
/// `entry` deliberately stops at the last line of content: blank lines and comment lines
/// between entries belong to no entry at all, which is why they survive. Deleting a key
/// removes `entry` and leaves the comment above it stranded but present — under "never
/// lose a byte", an orphaned comment beats a deleted one.
#[derive(Debug, Clone)]
pub struct Entry {
    pub key: String,
    pub entry: Range<usize>,
    pub value: Range<usize>,
    pub style: Style,
    pub indent: usize,
}

/// One `- …` element of a block sequence.
#[derive(Debug, Clone)]
pub struct Item {
    pub item: Range<usize>,
    /// The element's own content, after the dash — a nested mapping's first key, or a
    /// flow scalar. Indexing this recursively is what gives per-fact granularity.
    pub content: Range<usize>,
    pub indent: usize,
}

/// Multi-line state a line can leave behind for the next one.
#[derive(Debug, Clone, Copy, Default)]
struct Carry {
    quote: Option<u8>,
    depth: i32,
}

impl Carry {
    fn open(&self) -> bool {
        self.quote.is_some() || self.depth > 0
    }
}

/// What one line contains, given the state the previous line left.
struct Scanned {
    /// Byte offset of the `:` separating a key from its value, when there is one at
    /// depth zero. YAML only treats a colon as a separator when a space or the line end
    /// follows it, which is what lets `12:30` and `http://x` stay ordinary scalars.
    colon: Option<usize>,
    /// Byte offset where a trailing `# …` begins.
    comment: Option<usize>,
    carry: Carry,
}

fn scan_line(line: &str, mut carry: Carry) -> Scanned {
    let b = line.as_bytes();
    let mut i = 0;
    let mut colon = None;
    let mut comment = None;

    while i < b.len() {
        match carry.quote {
            Some(q @ b'\'') => {
                if b[i] == q {
                    // `''` is an escaped quote inside a single-quoted scalar.
                    if b.get(i + 1) == Some(&q) {
                        i += 2;
                        continue;
                    }
                    carry.quote = None;
                }
                i += 1;
            }
            Some(q @ b'"') => {
                if b[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if b[i] == q {
                    carry.quote = None;
                }
                i += 1;
            }
            Some(_) => unreachable!("only ' and \" open a quoted scalar"),
            None => {
                match b[i] {
                    b'\'' | b'"' => carry.quote = Some(b[i]),
                    b'#' if i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t' => {
                        comment = Some(i);
                        break;
                    }
                    b'[' | b'{' => carry.depth += 1,
                    b']' | b'}' => carry.depth -= 1,
                    b':' if carry.depth == 0 && colon.is_none() => {
                        let next = b.get(i + 1);
                        if matches!(
                            next,
                            None | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')
                        ) {
                            colon = Some(i);
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
    }

    if carry.depth < 0 {
        carry.depth = 0;
    }
    Scanned { colon, comment, carry }
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with('#')
}

/// A block scalar header — `|`, `>`, `|-`, `>2-` — introduces lines that are content no
/// matter what they look like.
fn opens_block_scalar(inline: &str) -> bool {
    let mut chars = inline.chars();
    match chars.next() {
        Some('|') | Some('>') => chars.all(|c| c.is_ascii_digit() || c == '-' || c == '+'),
        _ => false,
    }
}

/// Walk the lines of `region`, handing each one to `f` along with its absolute range and
/// whether it begins a new construct at `indent`.
///
/// Shared by the mapping and sequence indexers because the hard part — knowing when a
/// line is structure and when it is somebody's multi-line value — is identical.
fn walk(
    text: &str,
    region: Range<usize>,
    indent: usize,
    mut starts: impl FnMut(&str) -> bool,
) -> Option<Vec<(Range<usize>, bool)>> {
    let mut out: Vec<(Range<usize>, bool)> = Vec::new();
    let mut carry = Carry::default();
    let mut block_scalar: Option<usize> = None;
    let mut at = region.start;

    // A region can begin part-way along a line — a fact's mapping starts after its
    // `- `, not at column zero — so the first line's own indentation is short by
    // however far into the line the region started.
    let mut column =
        text[..region.start].rfind('\n').map_or(region.start, |n| region.start - n - 1);

    for line in text[region.start..region.end].split_inclusive('\n') {
        let range = at..at + line.len();
        at = range.end;
        let bare = line.trim_end_matches(['\n', '\r']);
        let here = column + indent_of(bare);
        column = 0;

        if let Some(parent) = block_scalar {
            if is_blank(bare) || here > parent {
                out.push((range, false));
                continue;
            }
            block_scalar = None;
        }

        if carry.open() {
            carry = scan_line(bare, carry).carry;
            out.push((range, false));
            continue;
        }

        if is_blank(bare) || is_comment(bare) {
            out.push((range, false));
            continue;
        }

        if bare.starts_with('\t') {
            return None; // tab indentation: not ours to interpret
        }

        let new = here == indent && starts(bare.trim_start());
        let scanned = scan_line(bare, carry);
        carry = scanned.carry;

        if let Some(colon) = scanned.colon {
            let inline = bare[colon + 1..].trim_start();
            let inline = match scanned.comment {
                Some(c) if c > colon => bare[colon + 1..c].trim(),
                _ => inline,
            };
            if opens_block_scalar(inline) {
                block_scalar = Some(here);
            }
        }

        out.push((range, new));
    }

    Some(out)
}

/// Index a block mapping. `None` means "I will not vouch for this" — the caller falls
/// back to a canonical rewrite.
pub fn index_block(text: &str, region: Range<usize>, indent: usize) -> Option<Vec<Entry>> {
    let lines = walk(text, region.clone(), indent, |trimmed| {
        !trimmed.starts_with('-') && scan_line(trimmed, Carry::default()).colon.is_some()
    })?;

    let mut starts: Vec<usize> = Vec::new();
    for (i, (_, new)) in lines.iter().enumerate() {
        if *new {
            starts.push(i);
        }
    }

    let mut entries = Vec::with_capacity(starts.len());
    for (n, &first) in starts.iter().enumerate() {
        let limit = starts.get(n + 1).copied().unwrap_or(lines.len());
        // Trailing blanks and the comments introducing the next key belong to neither.
        let mut last = first;
        for i in first..limit {
            let bare = text[lines[i].0.start..lines[i].0.end].trim_end_matches(['\n', '\r']);
            let trailing_gap = is_blank(bare) || (i > first && is_comment(bare));
            if !trailing_gap {
                last = i;
            }
        }

        let head = lines[first].0.clone();
        let bare = text[head.start..head.end].trim_end_matches(['\n', '\r']);
        let scanned = scan_line(bare, Carry::default());
        let colon = scanned.colon?;
        let key = unquote(bare[..colon].trim());

        let entry = head.start..lines[last].0.end;
        let inline_from = head.start + colon + 1;
        let inline_to = head.start + scanned.comment.unwrap_or(bare.len());
        let inline = text.get(inline_from..inline_to)?;
        let lead = inline.len() - inline.trim_start().len();
        let trimmed = inline.trim();

        let value = if trimmed.is_empty() {
            // The value is the indented block underneath.
            let body_start = head.end.min(entry.end);
            body_start..entry.end
        } else if last > first {
            // A flow collection or a block scalar carrying on past this line.
            (inline_from + lead)..entry.end
        } else {
            (inline_from + lead)..(inline_from + lead + trimmed.len())
        };

        let style = match text.as_bytes().get(value.start) {
            Some(b'{') | Some(b'[') => Style::Flow,
            _ => Style::Block,
        };

        entries.push(Entry { key, entry, value, style, indent });
    }

    Some(entries)
}

/// Index a block sequence's elements.
pub fn index_items(text: &str, region: Range<usize>, indent: usize) -> Option<Vec<Item>> {
    let lines =
        walk(text, region.clone(), indent, |trimmed| trimmed == "-" || trimmed.starts_with("- "))?;

    let mut starts: Vec<usize> = Vec::new();
    for (i, (_, new)) in lines.iter().enumerate() {
        if *new {
            starts.push(i);
        }
    }

    let mut items = Vec::with_capacity(starts.len());
    for (n, &first) in starts.iter().enumerate() {
        let limit = starts.get(n + 1).copied().unwrap_or(lines.len());
        let mut last = first;
        for i in first..limit {
            let bare = text[lines[i].0.start..lines[i].0.end].trim_end_matches(['\n', '\r']);
            let trailing_gap = is_blank(bare) || (i > first && is_comment(bare));
            if !trailing_gap {
                last = i;
            }
        }

        let head = lines[first].0.clone();
        let bare = text[head.start..head.end].trim_end_matches(['\n', '\r']);
        let after_dash = bare[indent + 1..].len() - bare[indent + 1..].trim_start().len();
        let content_start = head.start + indent + 1 + after_dash;

        items.push(Item {
            item: head.start..lines[last].0.end,
            content: content_start..lines[last].0.end,
            indent: indent + 1 + after_dash,
        });
    }

    Some(items)
}

fn unquote(key: &str) -> String {
    let bytes = key.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[bytes.len() - 1] == bytes[0]
    {
        return key[1..key.len() - 1].to_string();
    }
    key.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(text: &str) -> Vec<String> {
        index_block(text, 0..text.len(), 0).expect("indexes").into_iter().map(|e| e.key).collect()
    }

    fn value_of<'a>(text: &'a str, key: &str) -> &'a str {
        let e = index_block(text, 0..text.len(), 0)
            .expect("indexes")
            .into_iter()
            .find(|e| e.key == key)
            .expect("key is present");
        &text[e.value.start..e.value.end]
    }

    #[test]
    fn finds_every_top_level_key_in_order() {
        let text = "id: place_marrow\nname: Marrow\ntype: city\n";
        assert_eq!(keys(text), ["id", "name", "type"]);
        assert_eq!(value_of(text, "name"), "Marrow");
    }

    #[test]
    fn a_trailing_comment_is_not_part_of_the_value() {
        let text = "population: 9000  # before the siege\n";
        assert_eq!(value_of(text, "population"), "9000");
    }

    #[test]
    fn a_comment_between_keys_belongs_to_no_key() {
        let text = "id: x\n# why this is here\nname: Marrow\n";
        let entries = index_block(text, 0..text.len(), 0).expect("indexes");
        let id = entries.iter().find(|e| e.key == "id").unwrap();
        assert_eq!(&text[id.entry.start..id.entry.end], "id: x\n", "the comment is outside");
    }

    #[test]
    fn an_inline_flow_mapping_is_one_value() {
        let text = "existence: { from: \"0602~\", to: \"0812\" }\nname: Marrow\n";
        assert_eq!(value_of(text, "existence"), "{ from: \"0602~\", to: \"0812\" }");
        let e = index_block(text, 0..text.len(), 0).unwrap();
        assert_eq!(e[0].style, Style::Flow);
        assert_eq!(e[1].style, Style::Block);
    }

    #[test]
    fn a_nested_block_is_the_whole_value() {
        let text = "facts:\n  - attr: population\n    value: 9000\nname: Marrow\n";
        assert_eq!(keys(text), ["facts", "name"]);
        assert_eq!(value_of(text, "facts"), "  - attr: population\n    value: 9000\n");
    }

    /// The failure this scanner exists to not have. Prose below a `|` can contain
    /// anything, including something that reads exactly like a key at column zero.
    #[test]
    fn a_block_scalar_containing_a_colon_is_never_mistaken_for_a_key() {
        let text = "note: |\n  id: not a key\n  name: also not a key\nreal: yes\n";
        assert_eq!(keys(text), ["note", "real"]);
    }

    #[test]
    fn a_colon_inside_a_quoted_scalar_is_not_a_separator() {
        let text = "name: \"Marrow: the wall town\"\nid: place_marrow\n";
        assert_eq!(keys(text), ["name", "id"]);
        assert_eq!(value_of(text, "name"), "\"Marrow: the wall town\"");
    }

    #[test]
    fn a_flow_sequence_spanning_lines_stays_one_value() {
        let text = "shape: [\n  [0.1, 0.2],\n  [0.3, 0.4]\n]\nid: x\n";
        assert_eq!(keys(text), ["shape", "id"]);
        assert!(value_of(text, "shape").ends_with("]\n"));
    }

    #[test]
    fn a_time_of_day_is_not_a_key_separator() {
        let text = "at: 12:30\n";
        assert_eq!(keys(text), ["at"]);
        assert_eq!(value_of(text, "at"), "12:30");
    }

    #[test]
    fn sequence_items_are_found_with_their_own_indent() {
        let text =
            "  - attr: population\n    value: 9000\n  - attr: owner\n    value: pol_vashen\n";
        let items = index_items(text, 0..text.len(), 2).expect("indexes");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].indent, 4, "the nested mapping starts after the dash and a space");
        assert!(text[items[0].item.start..items[0].item.end].contains("population"));
        assert!(text[items[1].item.start..items[1].item.end].contains("pol_vashen"));
    }

    #[test]
    fn an_item_can_be_indexed_again_as_a_mapping() {
        let text = "  - attr: population\n    value: 9000\n";
        let items = index_items(text, 0..text.len(), 2).unwrap();
        let inner = index_block(text, items[0].content.clone(), items[0].indent).unwrap();
        assert_eq!(inner.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["attr", "value"]);
        assert_eq!(&text[inner[1].value.start..inner[1].value.end], "9000");
    }
}
