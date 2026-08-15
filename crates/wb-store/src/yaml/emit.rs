//! Writing back the handful of shapes a record can hold, in the style it already used.
//!
//! Two rules here are not cosmetic, and both were found by asking what happens when the
//! file is read again:
//!
//! - **An integral float keeps its `.0`.** `Value` is `#[serde(untagged)]`, so a `2.0`
//!   emitted as `2` comes back as `Value::Int(2)`. The record would round-trip into a
//!   different type than it went out as, silently, and comparisons and sorting would
//!   change meaning from then on.
//! - **A string that looks like something else is quoted.** A settlement named `No` is
//!   a boolean if you let it be, and a population written bare as `9000` after being
//!   typed as text would come back an integer.
//!
//! Dates are always double-quoted. Every date in the shipped example world is, the
//! grammar is full of characters YAML would rather interpret (`~`, `@`, `>`, `<`, `?`),
//! and quoting uniformly means the writer never has to think about which ones.

use wb_core::DateExpr;

use crate::model::{Fact, Span, Value};

use super::scan::Style;

/// A scalar as YAML, quoted only when leaving it bare would change what it means.
pub fn scalar(text: &str) -> String {
    if needs_quoting(text) { quoted(text) } else { text.to_string() }
}

pub fn value(v: &Value) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => float(*f),
        Value::Text(t) => scalar(t),
    }
}

/// A float that will read back as a float.
pub fn float(f: f64) -> String {
    let s = format!("{f}");
    if s.contains(['.', 'e', 'E']) || !s.chars().any(|c| c.is_ascii_digit()) {
        s
    } else {
        format!("{s}.0")
    }
}

pub fn date(d: &DateExpr) -> String {
    quoted(&d.to_string())
}

pub fn point(p: [f64; 2]) -> String {
    format!("[{}, {}]", float(p[0]), float(p[1]))
}

/// A span, in whichever style the file was already using.
pub fn span(s: &Span, style: Style, indent: usize) -> String {
    let ends: Vec<(&str, &DateExpr)> = [("from", &s.from), ("to", &s.to)]
        .into_iter()
        .filter(|(_, d)| **d != DateExpr::Unknown)
        .collect();

    // An end nobody has stated is left out rather than written as `?`. Both mean the
    // same thing to the loader, and one of them implies the writer owes an answer.
    match style {
        Style::Flow => {
            if ends.is_empty() {
                return "{}".to_string();
            }
            let inner: Vec<String> =
                ends.iter().map(|(k, d)| format!("{k}: {}", date(d))).collect();
            format!("{{ {} }}", inner.join(", "))
        }
        Style::Block => {
            let pad = " ".repeat(indent + 2);
            ends.iter().map(|(k, d)| format!("{pad}{k}: {}\n", date(d))).collect()
        }
    }
}

/// A list of bare ids — `parents`, `participants`.
pub fn ids(list: &[String], style: Style, indent: usize) -> String {
    match style {
        Style::Flow => {
            let inner: Vec<String> = list.iter().map(|s| scalar(s)).collect();
            format!("[{}]", inner.join(", "))
        }
        Style::Block => {
            let pad = " ".repeat(indent + 2);
            list.iter().map(|s| format!("{pad}- {}\n", scalar(s))).collect()
        }
    }
}

pub fn points(list: &[[f64; 2]], style: Style, indent: usize) -> String {
    match style {
        Style::Flow => {
            let inner: Vec<String> = list.iter().map(|p| point(*p)).collect();
            format!("[{}]", inner.join(", "))
        }
        Style::Block => {
            let pad = " ".repeat(indent + 2);
            list.iter().map(|p| format!("{pad}- {}\n", point(*p))).collect()
        }
    }
}

/// One fact as a sequence element, at the given sequence indent.
pub fn fact_item(f: &Fact, indent: usize) -> String {
    let pad = " ".repeat(indent);
    let inner = " ".repeat(indent + 2);
    let mut out = format!("{pad}- attr: {}\n{inner}value: {}\n", scalar(&f.attr), value(&f.value));
    if f.from != DateExpr::Unknown {
        out.push_str(&format!("{inner}from: {}\n", date(&f.from)));
    }
    if f.to != DateExpr::Unknown {
        out.push_str(&format!("{inner}to: {}\n", date(&f.to)));
    }
    out
}

pub fn facts(list: &[Fact], indent: usize) -> String {
    list.iter().map(|f| fact_item(f, indent + 2)).collect()
}

fn quoted(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
    format!("\"{escaped}\"")
}

/// Would this text mean something other than itself if written bare?
fn needs_quoting(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    if text != text.trim() {
        return true;
    }
    if reads_as_something_else(text) {
        return true;
    }
    if text.contains(": ") || text.ends_with(':') || text.contains(" #") || text.contains('\n') {
        return true;
    }
    let first = text.as_bytes()[0];
    if b"#&*!|>'\"%@`,[]{}".contains(&first) {
        return true;
    }
    // A leading dash is only trouble when a sequence entry could be read out of it.
    text == "-" || text.starts_with("- ")
}

fn reads_as_something_else(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "nil" | "~" | ".nan" | ".inf"
    ) {
        return true;
    }
    text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_number_float_keeps_its_point_so_it_stays_a_float() {
        assert_eq!(float(2.0), "2.0");
        assert_eq!(float(-1.0), "-1.0");
        assert_eq!(float(0.43), "0.43");
        assert_eq!(value(&Value::Float(2.0)), "2.0");
        assert_eq!(value(&Value::Int(2)), "2");
    }

    /// The round trip is the actual claim, since `Value` is untagged and will happily
    /// come back as whichever variant the text suggests.
    #[test]
    fn every_value_reads_back_as_the_kind_it_was_written_as() {
        for v in [
            Value::Int(9000),
            Value::Float(2.0),
            Value::Float(0.5),
            Value::Bool(true),
            Value::Text("pol_vashen".into()),
            Value::Text("9000".into()),
            Value::Text("2.0".into()),
            Value::Text("No".into()),
            Value::Text("true".into()),
            Value::Text("#B07A2B".into()),
            Value::Text("Duke of Corrath".into()),
            Value::Text("".into()),
            Value::Text("~".into()),
        ] {
            let yaml = format!("v: {}\n", value(&v));
            let back: std::collections::BTreeMap<String, Value> =
                serde_yaml_bw::from_str(&yaml).unwrap_or_else(|e| panic!("{yaml:?}: {e}"));
            assert_eq!(back["v"], v, "round trip of {v:?} through {yaml:?}");
        }
    }

    #[test]
    fn a_text_value_that_looks_like_a_number_is_quoted() {
        assert_eq!(value(&Value::Text("9000".into())), "\"9000\"");
        assert_eq!(value(&Value::Text("0.5".into())), "\"0.5\"");
        assert_eq!(value(&Value::Text("pol_vashen".into())), "pol_vashen");
    }

    #[test]
    fn a_place_called_no_is_a_place_and_not_a_boolean() {
        assert_eq!(scalar("No"), "\"No\"");
        assert_eq!(scalar("Marrow"), "Marrow");
        assert_eq!(scalar("The Vale of Corrath"), "The Vale of Corrath");
    }

    #[test]
    fn a_colour_is_quoted_because_a_hash_starts_a_comment() {
        assert_eq!(scalar("#B07A2B"), "\"#B07A2B\"");
    }

    #[test]
    fn an_unstated_end_is_left_out_rather_than_written_as_a_question_mark() {
        let s = Span { from: wb_core::parse_date("0602~").unwrap(), to: DateExpr::Unknown };
        assert_eq!(span(&s, Style::Flow, 0), "{ from: \"0602~\" }");

        let both = Span { from: DateExpr::Unknown, to: DateExpr::Unknown };
        assert_eq!(span(&both, Style::Flow, 0), "{}");
    }

    #[test]
    fn a_span_stays_in_the_style_it_was_written_in() {
        let s = Span {
            from: wb_core::parse_date("0602~").unwrap(),
            to: wb_core::parse_date("0812").unwrap(),
        };
        assert_eq!(span(&s, Style::Flow, 0), "{ from: \"0602~\", to: \"0812\" }");
        assert_eq!(span(&s, Style::Block, 0), "  from: \"0602~\"\n  to: \"0812\"\n");
    }

    #[test]
    fn a_fact_with_no_dates_carries_no_date_keys() {
        let f = Fact {
            attr: "title".into(),
            value: Value::Text("Duke of Corrath".into()),
            from: DateExpr::Unknown,
            to: DateExpr::Unknown,
        };
        assert_eq!(fact_item(&f, 2), "  - attr: title\n    value: Duke of Corrath\n");
    }

    #[test]
    fn geometry_is_written_as_pairs() {
        assert_eq!(point([0.43, 0.4]), "[0.43, 0.4]");
        assert_eq!(points(&[[0.1, 0.2], [0.3, 0.4]], Style::Flow, 0), "[[0.1, 0.2], [0.3, 0.4]]");
        assert_eq!(
            points(&[[0.1, 0.2]], Style::Block, 0),
            "  - [0.1, 0.2]\n",
            "one vertex per line, the way the example world writes a shape"
        );
    }
}
