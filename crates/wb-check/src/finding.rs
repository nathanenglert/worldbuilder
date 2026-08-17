use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use wb_core::Day;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// An event names someone or somewhere that did not exist when it happened.
    ExistenceViolation,
    /// A fact points at something that did not exist while the fact held.
    AnachronisticFact,
    /// One attribute asserted two ways over the same days.
    ConflictingFacts,
    /// A reference to an id nothing defines.
    OrphanReference,
    /// A single-valued attribute with a hole nothing fills.
    SuccessionGap,
    /// A child born before their parent, or too long after them.
    ImpossibleParentage,
    /// A scene's prose names someone who was not alive when the scene is set.
    ///
    /// The one rule whose body does not live here: it needs the manuscript, and
    /// `wb-check` reads no files. `wb-story::canon` produces it. The name lives here so
    /// a contradiction found on the page renders exactly like one found in the records.
    SceneContradiction,
}

impl Rule {
    pub fn slug(self) -> &'static str {
        match self {
            Self::ExistenceViolation => "existence-violation",
            Self::AnachronisticFact => "anachronistic-fact",
            Self::ConflictingFacts => "conflicting-facts",
            Self::OrphanReference => "orphan-reference",
            Self::SuccessionGap => "succession-gap",
            Self::ImpossibleParentage => "impossible-parentage",
            Self::SceneContradiction => "scene-contradiction",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::ExistenceViolation => "Existence violation",
            Self::AnachronisticFact => "Anachronistic reference",
            Self::ConflictingFacts => "Conflicting facts",
            Self::OrphanReference => "Orphan reference",
            Self::SuccessionGap => "Succession gap",
            Self::ImpossibleParentage => "Impossible parentage",
            Self::SceneContradiction => "Scene contradiction",
        }
    }
}

/// Whether the world's own vagueness leaves any reading in which this is fine.
///
/// The distinction is the point of the whole engine. A `Definite` finding is wrong
/// under every reading of every fuzzy date and wants fixing. A `Possible` one is the
/// shape a deliberate mystery takes, and the writer decides which it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    Possible,
    Definite,
}

impl Certainty {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Possible => "possible",
            Self::Definite => "definite",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub rule: Rule,
    pub certainty: Certainty,
    /// The record a writer would open first.
    pub subject: String,
    pub related: Vec<String>,
    pub message: String,
    /// Where on the timeline to jump, when the finding has a position.
    pub at: Option<Day>,
    pub sources: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// Wrong under every reading.
    pub fn definite(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.certainty == Certainty::Definite)
    }

    /// Wrong under some readings — mysteries live here too.
    pub fn possible(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.certainty == Certainty::Possible)
    }

    pub fn of_rule(&self, rule: Rule) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(move |f| f.rule == rule)
    }

    pub fn counts(&self) -> (usize, usize) {
        (self.definite().count(), self.possible().count())
    }
}
