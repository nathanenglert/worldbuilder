//! Finding the world inside the prose.
//!
//! This is the component that can be *confidently wrong*, which is a worse failure than
//! being unable to answer. A writer shown "31% of your world reaches the page" will act
//! on that number; if it is inflated by a common noun matching a place name, they will
//! act on it in the wrong direction. So every decision here is taken conservatively and
//! every hit is auditable back to the sentence it came from.
//!
//! # What counts
//!
//! An entity's `name`, each of its `aka` entries, and any `[[wikilink]]` naming it —
//! matched case-insensitively on whole words. That is all. In particular, single words
//! from a multi-word name are **not** matched automatically: "The Vale of Corrath" would
//! then be found in every sentence containing "vale", and a writer with a place called
//! The Rise or The Gate would get a ratio built almost entirely of false positives. A
//! writer who wants "Aldric" to count writes `aka: [Aldric]`, which is one line and is
//! honest about being a decision.
//!
//! # Why not `World::search`
//!
//! It runs the other way — one query against every record — so scanning a chapter with
//! it costs one full pass per entity, and it matches on `contains` with no word
//! boundaries. Both are right for a search box and wrong for this.
//!
//! The scan here walks the prose's words **once**, testing windows of 1..=K words at
//! each position, where K is the longest alias in the world. That is O(words × K) and
//! independent of how many records exist — which is what keeps it usable on a world of
//! ten thousand records against a novel.

use std::collections::HashMap;

use wb_store::World;

/// How the prose named the entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// The record's own `name`.
    Name,
    /// Something in its `aka` list.
    Alias,
    /// A `[[wikilink]]`, which counts however it is spelled.
    Wikilink,
}

#[derive(Debug, Clone)]
pub struct Mention {
    pub id: String,
    pub via: Via,
    /// Byte offsets into the text that was scanned.
    pub at: usize,
    pub len: usize,
}

/// Every way this world can be named, keyed by lowercased whole words.
#[derive(Debug, Clone, Default)]
pub struct Index {
    /// `"the vale"` → (`ter_vale_of_corrath`, how it was spelled).
    by_phrase: HashMap<String, (String, Via)>,
    /// Ids, for `[[place_marrow]]`.
    by_id: HashMap<String, String>,
    longest: usize,
}

impl Index {
    pub fn is_empty(&self) -> bool {
        self.by_phrase.is_empty()
    }
}

/// Build the lookup once per world.
///
/// A name and an alias colliding is resolved in favour of the name: two records that
/// answer to the same phrase is a real thing in a real world ("the Duke"), and silently
/// picking the second one loaded is worse than consistently picking the named one.
pub fn index(world: &World) -> Index {
    let mut out = Index::default();

    for entity in world.entities.values() {
        out.by_id.insert(entity.id.to_lowercase(), entity.id.clone());

        for (via, text) in std::iter::once((Via::Name, &entity.name))
            .chain(entity.aliases.iter().map(|a| (Via::Alias, a)))
        {
            let words = normalize(text);
            if words.is_empty() {
                continue;
            }
            out.longest = out.longest.max(words.len());
            let phrase = words.join(" ");

            match out.by_phrase.get(&phrase) {
                // A name already claimed it; an alias does not get to take it away.
                Some((_, Via::Name)) if via == Via::Alias => {}
                _ => {
                    out.by_phrase.insert(phrase, (entity.id.clone(), via));
                }
            }
        }
    }

    out
}

/// Every mention in `text`, in the order they appear.
pub fn scan(index: &Index, text: &str) -> Vec<Mention> {
    let mut out = Vec::new();
    if index.is_empty() {
        return out;
    }

    let links = wikilinks(index, text, &mut out);
    let words = words_of(text, &links);

    let mut i = 0;
    while i < words.len() {
        // Longest first, so "The Vale of Corrath" wins over the "the Vale" inside it.
        let mut matched = 0;
        for n in (1..=index.longest.min(words.len() - i)).rev() {
            let phrase =
                words[i..i + n].iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
            if let Some((id, via)) = index.by_phrase.get(&phrase) {
                out.push(Mention {
                    id: id.clone(),
                    via: *via,
                    at: words[i].at,
                    len: words[i + n - 1].at + words[i + n - 1].len - words[i].at,
                });
                matched = n;
                break;
            }
        }
        i += matched.max(1);
    }

    out.sort_by_key(|m| m.at);
    out
}

/// How many separate records the prose names.
pub fn distinct(mentions: &[Mention]) -> Vec<String> {
    let mut ids: Vec<String> = mentions.iter().map(|m| m.id.clone()).collect();
    ids.sort();
    ids.dedup();
    ids
}

/// The sentence a mention sits in, for showing the writer why it counted.
///
/// Sentence-bounded rather than a fixed character window: the point of an excerpt here
/// is to let someone judge whether a match is real, and half a clause either side of a
/// word does not let them.
pub fn excerpt(text: &str, at: usize, len: usize) -> String {
    let is_break = |c: char| matches!(c, '.' | '!' | '?' | '\n');

    let start = text[..at].rfind(is_break).map(|i| i + 1).unwrap_or(0);
    let end = text[at + len..].find(is_break).map(|i| at + len + i + 1).unwrap_or(text.len());

    text[start..end].split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------- internals

struct Word {
    text: String,
    at: usize,
    len: usize,
}

/// Lowercased alphanumeric runs. Punctuation and apostrophes split, so "Aldric's" yields
/// "aldric" and matches — which is what a reader would say it does.
fn normalize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn words_of(text: &str, skip: &[std::ops::Range<usize>]) -> Vec<Word> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;

    let push = |out: &mut Vec<Word>, from: usize, to: usize| {
        if skip.iter().any(|r| from < r.end && r.start < to) {
            return;
        }
        out.push(Word { text: text[from..to].to_lowercase(), at: from, len: to - from });
    };

    for (i, c) in text.char_indices() {
        match (c.is_alphanumeric(), start) {
            (true, None) => start = Some(i),
            (false, Some(from)) => {
                push(&mut out, from, i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        push(&mut out, from, text.len());
    }
    out
}

/// `[[Marrow]]` and `[[place_marrow|Marrow]]`, which count however they are spelled.
///
/// Returns the byte ranges consumed, so the word scan does not count the same reference
/// twice — once as a link and once as the plain words inside it.
fn wikilinks(index: &Index, text: &str, out: &mut Vec<Mention>) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let mut i = 0;

    while let Some(open) = text[i..].find("[[") {
        let open = i + open;
        let Some(close) = text[open + 2..].find("]]") else { break };
        let close = open + 2 + close;
        i = close + 2;

        let inner = &text[open + 2..close];
        // `[[target|what the sentence says]]` — the target is what it points at.
        let target = inner.split('|').next().unwrap_or(inner).trim();

        let resolved = index
            .by_id
            .get(&target.to_lowercase())
            .or_else(|| index.by_phrase.get(&normalize(target).join(" ")).map(|(id, _)| id))
            .cloned();

        if let Some(id) = resolved {
            out.push(Mention { id, via: Via::Wikilink, at: open, len: close + 2 - open });
            ranges.push(open..close + 2);
        }
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use wb_core::{Calendar, Fuzz, Month};
    use wb_store::{Entity, Rules, WorldDef};

    fn entity(id: &str, name: &str, aka: &[&str]) -> Entity {
        Entity {
            id: id.into(),
            name: name.into(),
            aliases: aka.iter().map(|a| (*a).to_string()).collect(),
            type_name: "thing".into(),
            existence: None,
            parents: Vec::new(),
            facts: Vec::new(),
            marker: None,
            shape: Vec::new(),
            body: String::new(),
            source: PathBuf::from(format!("{id}.md")),
        }
    }

    fn world(entities: Vec<Entity>) -> World {
        let def = WorldDef {
            name: "Test".into(),
            calendar: Calendar::new(
                "T",
                (1..=12).map(|i| Month::new(format!("M{i}"), 30)).collect(),
            )
            .unwrap(),
            map: None,
            manuscript: None,
            fuzz: Fuzz::default(),
            types: Vec::new(),
            rules: Rules::default(),
        };
        World::assemble(PathBuf::from("."), def, entities, Vec::new(), Vec::new()).unwrap()
    }

    fn ids(index: &Index, text: &str) -> Vec<String> {
        scan(index, text).into_iter().map(|m| m.id).collect()
    }

    /// The false positive the whole matching strategy exists to prevent. If this ever
    /// passes by matching, the iceberg ratio becomes a number nobody should act on.
    #[test]
    fn a_multi_word_name_is_never_found_by_one_of_its_words() {
        let w = world(vec![entity("ter_vale", "The Vale of Corrath", &[])]);
        let index = index(&w);

        assert_eq!(ids(&index, "The Vale of Corrath is green."), vec!["ter_vale"]);
        assert!(ids(&index, "They rode down into the vale at dusk.").is_empty());
        assert!(ids(&index, "An avalanche took the road.").is_empty());
    }

    /// And the escape hatch works: one declared line makes the short form count.
    #[test]
    fn an_alias_is_what_makes_the_short_form_count() {
        let w = world(vec![entity("ter_vale", "The Vale of Corrath", &["the Vale"])]);
        let index = index(&w);

        assert_eq!(ids(&index, "the Vale went on being green"), vec!["ter_vale"]);
        assert!(ids(&index, "down into the vale at dusk").len() == 1, "declared, so it counts");
    }

    #[test]
    fn the_longest_name_wins_over_the_shorter_one_inside_it() {
        let w = world(vec![
            entity("ter_vale", "The Vale of Corrath", &["the Vale"]),
            entity("pol_corrath", "Corrath", &[]),
        ]);
        let index = index(&w);

        // One mention, not three: not "the Vale" and not a bare "Corrath" as well.
        let hits = scan(&index, "The Vale of Corrath held out.");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "ter_vale");
    }

    #[test]
    fn matching_ignores_case_and_stops_at_word_boundaries() {
        let w = world(vec![entity("place_marrow", "Marrow", &[])]);
        let index = index(&w);

        assert_eq!(ids(&index, "MARROW held."), vec!["place_marrow"]);
        assert_eq!(ids(&index, "marrow, and then"), vec!["place_marrow"]);
        assert!(ids(&index, "bone marrowed shut").is_empty(), "not inside another word");
    }

    #[test]
    fn a_possessive_still_names_the_person() {
        let w = world(vec![entity("act_aldric", "Aldric Vane", &["Aldric"])]);
        let index = index(&w);
        assert_eq!(ids(&index, "Aldric's cloak"), vec!["act_aldric"]);
    }

    #[test]
    fn a_wikilink_counts_even_when_the_display_text_differs() {
        let w = world(vec![entity("place_marrow", "Marrow", &[])]);
        let index = index(&w);

        assert_eq!(ids(&index, "held at [[place_marrow|the wall town]]"), vec!["place_marrow"]);
        assert_eq!(ids(&index, "held at [[Marrow]]"), vec!["place_marrow"]);
        assert!(ids(&index, "held at [[somewhere else]]").is_empty());
    }

    /// A link and the words inside it are one reference, not two.
    #[test]
    fn a_wikilink_is_not_counted_twice() {
        let w = world(vec![entity("place_marrow", "Marrow", &[])]);
        let index = index(&w);
        assert_eq!(scan(&index, "[[Marrow]] again").len(), 1);
    }

    #[test]
    fn a_world_with_nothing_in_it_scans_to_nothing() {
        let index = index(&world(Vec::new()));
        assert!(index.is_empty());
        assert!(scan(&index, "Marrow, Corrath, the Vale.").is_empty());
    }

    #[test]
    fn an_excerpt_is_the_whole_sentence_around_the_hit() {
        let text = "One thing happened. Aldric came up the stair. Another thing did.";
        let at = text.find("Aldric").unwrap();
        assert_eq!(excerpt(text, at, 6), "Aldric came up the stair.");
    }
}
