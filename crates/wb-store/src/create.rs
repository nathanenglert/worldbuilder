//! Making a world that does not exist yet.
//!
//! The app could open a world and never make one, so the first thing a local-first tool
//! asked of a writer was to leave it: read the format, hand-write a `world.yaml`, come
//! back. For a file whose only two required keys are a name and a calendar.
//!
//! What goes in that file is a *starting point*, not a template this app owns. It is
//! commented the way the example world's own `world.yaml` is commented, because for
//! everything the app cannot yet ask for — a map, a manuscript, the writer's own words
//! for the things in their world — the file **is** the interface. Each of those is in
//! there, set or waiting behind a `#`. A writer who never opens it has a working world;
//! a writer who does has the documentation.
//!
//! Nothing here writes a record. A new world is empty, and inventing three people to
//! populate somebody's fiction would be the one kind of help this tool must not offer.

use std::path::{Path, PathBuf};

use crate::atomic;
use crate::error::{Error, Result};
use crate::yaml::emit;

/// Which calendar a world starts out keeping time by.
///
/// Two arms, and the second one is not a calendar so much as somewhere to start
/// renaming: inventing time is work a writer does over weeks, and twelve thirty-day
/// months called `First` through `Twelfth` are unmistakably provisional, which is the
/// property that matters. Historical and contemporary fiction wants the other arm and
/// should not have to invent anything at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timekeeping {
    Earth,
    Own,
}

impl Timekeeping {
    /// Parse the two words the frontend sends. Unknown spellings are an invented
    /// calendar, because that is the arm that assumes least about the writer's world.
    pub fn parse(word: &str) -> Self {
        if word.eq_ignore_ascii_case("earth") { Self::Earth } else { Self::Own }
    }
}

/// Write a new world into `root`, and hand back the `world.yaml` that now heads it.
///
/// `root` need not exist, and if it does it may be full of the writer's other files or
/// already be a git repository — none of that is this function's business. A folder
/// that already holds a `world.yaml` holds a *world*, though, and overwriting it would
/// make this the one operation in the app that can destroy one without asking.
pub fn scaffold(root: &Path, name: &str, time: Timekeeping) -> Result<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::UnnamedWorld);
    }

    let file = root.join("world.yaml");
    if file.exists() {
        return Err(Error::WorldExists { root: root.to_path_buf() });
    }

    // The three folders the loader looks in, and no more. `entities/actors` and its
    // three siblings are `paths::folder_for`'s business and appear when a record does —
    // a `polities/` made for a writer who has no polities is furniture, and an empty
    // directory is not something git would carry anywhere anyway.
    for dir in ["entities", "events", "scenes"] {
        let path = root.join(dir);
        std::fs::create_dir_all(&path).map_err(|source| Error::Io { path, source })?;
    }

    // Written whether or not this world is going under version control today, because
    // the version panel's advice to a world that is not is `git init` in that folder —
    // and by then nobody is thinking about a cache directory. The app already refuses to
    // stage `.worldbuilder/` itself; this is for the writer's own terminal, where a
    // folder of derived terrain otherwise shows up as forty untracked files.
    let ignore = root.join(".gitignore");
    if !ignore.exists() {
        atomic::write(&ignore, IGNORE)?;
    }

    atomic::write(&file, &world_yaml(name, time))?;
    Ok(file)
}

const IGNORE: &str = "\
# Derived from the files beside it and rebuilt whenever they change: the terrain worked
# out from your map, and the scratch copy of an older revision the app makes to compare
# two of them against each other. Nothing in here is canon.
.worldbuilder/
";

fn world_yaml(name: &str, time: Timekeeping) -> String {
    format!(
        "\
name: {name}

{calendar}
# How far a trailing `~` widens a date, in days, by the precision it was written at.
# `0811~` is a year that might be a couple either side of that one; `0811-04-12~` is a
# few days either way. A date nobody has pinned down yet is still a date here.
fuzz: {{ year: 730, month: 30, day: 3 }}

# Your own words for the things in this world.
#
# `primitive` is the half the app reasons about — an actor has parents and a lifespan, a
# polity holds territory over time, a place has geometry, a thing has neither and may
# outlast everyone. The name beside it is what *you* call one, so rename `faction` to
# `duchy`, `hive` or `covenant`, and add as many as the world turns out to need. A
# record whose `type` is not declared here still loads; it just has no primitive, so
# nothing knows to put it on the map or in a bloodline.
types:
  - {{ name: person, primitive: actor }}
  - {{ name: place, primitive: place }}
  - {{ name: faction, primitive: polity }}
  - {{ name: thing, primitive: thing }}

# The book, when there is one. It lives outside this folder on purpose: the prose belongs
# to Scrivener or Obsidian or Word, this app only ever reads it, and it is not versioned
# with the world. Point at it and the story panel can say which of these records ever
# reach the page — and which of them the reader has never met.
# manuscript:
#   root: ../manuscript

# A map you have drawn. Everything under `terrain:` is a dial you own; the coastline,
# rivers and biomes worked out from it are derived, cached under `.worldbuilder/`, and
# rebuilt whenever one of those numbers moves.
# map:
#   image: map/world.png
",
        name = emit::scalar(name),
        calendar = calendar(time),
    )
}

/// The calendar block, and the running commentary that makes it editable.
///
/// Two literal blocks rather than one built from [`wb_core::Calendar`]. What is being
/// written is a document — half of it is comments, and the comments differ by arm — and
/// a serializer would emit neither.
fn calendar(time: Timekeeping) -> &'static str {
    match time {
        Timekeeping::Earth => {
            "\
# Earth's calendar, because this world keeps Earth's time. Still yours: this is a file
# in your folder, not a setting in an application.
calendar:
  name: Gregorian
  months:
    - { name: January, days: 31 }
    - { name: February, days: 28 }
    - { name: March, days: 31 }
    - { name: April, days: 30 }
    - { name: May, days: 31 }
    - { name: June, days: 30 }
    - { name: July, days: 31 }
    - { name: August, days: 31 }
    - { name: September, days: 30 }
    - { name: October, days: 31 }
    - { name: November, days: 30 }
    - { name: December, days: 31 }
  weekdays: [Monday, Tuesday, Wednesday, Thursday, Friday, Saturday, Sunday]
  # February gains a day every fourth year, except every hundredth, except every four
  # hundredth. Delete the line and a year is simply the months added up.
  leap: { every: 4, except: 100, also: 400, month: 2, extra_days: 1 }
  # What `+2g` means in a date: two generations after whatever it is anchored to.
  generation_years: 25
  # Eras, if this world counts its years from something. The first year of an era is
  # year 1, so `starts: 0` makes the year written `0000` read as \"1 AR\".
  # eras:
  #   - { name: After the Reckoning, abbr: AR, starts: 0 }
"
        }
        Timekeeping::Own => {
            "\
# Time, as this world keeps it — and none of this is decided. Rename the months, give
# them different lengths, add or take some away; a year is however many days they come
# to. Everything is counted in days underneath, so every date in the world is re-read
# against whatever you make this.
calendar:
  name: Local Reckoning
  months:
    - { name: First, days: 30 }
    - { name: Second, days: 30 }
    - { name: Third, days: 30 }
    - { name: Fourth, days: 30 }
    - { name: Fifth, days: 30 }
    - { name: Sixth, days: 30 }
    - { name: Seventh, days: 30 }
    - { name: Eighth, days: 30 }
    - { name: Ninth, days: 30 }
    - { name: Tenth, days: 30 }
    - { name: Eleventh, days: 30 }
    - { name: Twelfth, days: 30 }
  # The days of the week, if this world has them. Empty is a world that does not.
  weekdays: []
  # What `+2g` means in a date: two generations after whatever it is anchored to.
  generation_years: 25
  # Eras, if this world counts its years from something. The first year of an era is
  # year 1, so `starts: 0` makes the year written `0000` read as \"1 AR\".
  # eras:
  #   - { name: After the Reckoning, abbr: AR, starts: 0 }
"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wb-create-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// The whole point, stated as the one property that matters: what this writes, the
    /// loader reads. A scaffolder that produced a `world.yaml` the app then refused to
    /// open would be worse than no scaffolder at all.
    #[test]
    fn a_new_world_opens_the_moment_it_is_written() {
        let root = scratch("opens");
        scaffold(&root, "The Vashen Reckoning", Timekeeping::Own).expect("scaffold");

        let world = load(&root).expect("load what was just written");
        assert_eq!(world.name, "The Vashen Reckoning");
        assert_eq!(world.entities.len(), 0, "nobody's fiction is invented for them");
        assert_eq!(world.events.len(), 0);
        // The four starter types are what stop a first record from having no primitive,
        // which is the difference between a record that appears on the map and one that
        // silently does not.
        assert_eq!(world.types.len(), 4);
    }

    #[test]
    fn a_world_that_keeps_earths_time_keeps_earths_leap_years() {
        let root = scratch("earth");
        scaffold(&root, "Nineteen Twenty-Nine", Timekeeping::Earth).expect("scaffold");

        let cal = &load(&root).expect("load").calendar;
        assert!(cal.is_leap(2024));
        assert!(!cal.is_leap(1900), "a hundredth year is not a leap year");
        assert!(cal.is_leap(2000), "except every four hundredth, which is");
        assert!(cal.audit_leap_rule(), "the divisors have to nest or the day count is wrong");
    }

    #[test]
    fn a_world_that_keeps_its_own_time_starts_from_something_plainly_provisional() {
        let root = scratch("own");
        scaffold(&root, "Somewhere Else", Timekeeping::Own).expect("scaffold");

        let cal = &load(&root).expect("load").calendar;
        assert_eq!(cal.months.len(), 12);
        assert_eq!(cal.months.iter().map(|m| m.days as i64).sum::<i64>(), 360);
        assert!(cal.weekdays.is_empty(), "a world that has not decided it has weeks");
    }

    /// A world is a folder the writer owns, so the folder may already have things in it.
    /// Everything except another world.
    #[test]
    fn a_folder_that_already_holds_a_world_is_refused_rather_than_overwritten() {
        let root = scratch("occupied");
        scaffold(&root, "The First One", Timekeeping::Own).expect("scaffold");
        std::fs::write(root.join("entities/somebody.yaml"), "id: act_a\nname: A\ntype: person\n")
            .expect("a record the writer has already written");

        let again = scaffold(&root, "The Second One", Timekeeping::Earth);
        assert!(matches!(again, Err(Error::WorldExists { .. })));
        assert_eq!(load(&root).expect("load").name, "The First One", "and it is untouched");
    }

    #[test]
    fn a_world_needs_a_name() {
        let root = scratch("unnamed");
        assert!(matches!(scaffold(&root, "   ", Timekeeping::Own), Err(Error::UnnamedWorld)));
        assert!(!root.join("world.yaml").exists(), "and nothing is left behind");
    }

    /// Worlds are named by novelists, and YAML has opinions about a name like `No`.
    ///
    /// The failure this closes is not a parse error — it is quieter than that. Written
    /// bare, `No` comes back as the boolean `false` and the world is called "false" from
    /// then on, in the header, in the export, and in every save point message.
    #[test]
    fn a_name_yaml_would_rather_read_as_something_else_survives_being_written() {
        for name in ["No", "Yes", "1984", "Sundown: A Reckoning"] {
            let root = scratch("named");
            scaffold(&root, name, Timekeeping::Own).expect("scaffold");
            assert_eq!(load(&root).expect("load").name, name);
        }
    }
}
