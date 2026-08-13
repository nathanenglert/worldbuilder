//! The shipped skills, checked against the surface they document.
//!
//! Skills are where worldbuilding *methodology* lives, and they are distributed
//! separately from the app — which means nothing else catches them drifting out of step
//! with the tools they call. A skill naming a tool that no longer exists fails silently,
//! at the worst moment, in someone else's session.

use std::fs;
use std::path::PathBuf;

use wb_mcp::WorldServer;

const EXPECTED: [&str; 6] = [
    "chapter-canon-check",
    "consistency-audit",
    "culture-from-phonology",
    "iceberg-check",
    "succession-crisis",
    "world-from-notes",
];

fn skills_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../skills")
}

struct Skill {
    dir: String,
    name: String,
    description: String,
    body: String,
}

fn skills() -> Vec<Skill> {
    let mut found: Vec<Skill> = fs::read_dir(skills_dir())
        .expect("skills/ exists")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|entry| {
            let dir = entry.file_name().to_string_lossy().to_string();
            let path = entry.path().join("SKILL.md");
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} has no readable SKILL.md: {e}", dir));

            let doc = wb_store::frontmatter::split(&text)
                .unwrap_or_else(|| panic!("`{dir}` has no YAML frontmatter"));
            let meta: serde_yaml_bw::Value = serde_yaml_bw::from_str(doc.frontmatter)
                .unwrap_or_else(|e| panic!("`{dir}` frontmatter does not parse: {e}"));

            let field = |key: &str| {
                meta.get(key)
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| panic!("`{dir}` frontmatter has no `{key}`"))
                    .to_string()
            };

            Skill {
                name: field("name"),
                description: field("description"),
                body: doc.body.to_string(),
                dir,
            }
        })
        .collect();
    found.sort_by(|a, b| a.dir.cmp(&b.dir));
    found
}

#[test]
fn every_shipped_skill_declares_itself_properly() {
    let found = skills();
    let dirs: Vec<&str> = found.iter().map(|s| s.dir.as_str()).collect();
    assert_eq!(dirs, EXPECTED);

    for skill in &found {
        assert_eq!(skill.name, skill.dir, "a skill's name must match its folder");
        assert!(
            skill.description.contains("Use when"),
            "`{}` never says when to use it, which is the only thing that gets it \
             loaded at the right moment",
            skill.dir
        );
        assert!(skill.body.len() > 400, "`{}` is a stub", skill.dir);
    }
}

/// Every tool must have at least one skill that knows what to do with it.
///
/// This runs in the direction that actually rots: adding a tool and forgetting the
/// methodology that makes it useful. A tool nothing knows how to use is a tool a model
/// will reach for at random.
#[test]
fn no_tool_ships_without_a_skill_that_uses_it() {
    let corpus: String = skills().iter().map(|s| s.body.clone()).collect::<Vec<_>>().join("\n");

    let orphans: Vec<String> = WorldServer::tool_names()
        .into_iter()
        .filter(|name| !corpus.contains(name.as_str()))
        .collect();

    assert!(orphans.is_empty(), "no shipped skill mentions: {orphans:?}");
}
