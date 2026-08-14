# Worldbuilder

A local-first worldbuilding tool for novelists and game designers, built on one idea:

**The map is a projection of the timeline.**

Nothing in a world is a scalar. A city's population, a region's owner, a person's title —
each is an assertion that held over a window of days. Ask what was true in 812 and you
get a map; drag the scrubber and kingdoms rise, borders move, and people are born and
die, because every one of those is the same query at a different instant.

Your data is files you own — Markdown with frontmatter, YAML for structure — in a folder
you can put under git, edit in any editor, and read without this application.

## What makes it different

**Vagueness is data, not a gap to fill.** `0812~` means *about* then. `0810..0815` means
somewhere in there. `@evt_siege_of_marrow+1y` means a year after the siege, and moving
the siege moves it. `?` means nobody knows, and is a perfectly good answer. A border
between two claims nobody dated is drawn hatched and dashed rather than assigned to
whoever the code happened to check first.

**The ground is derived, and it is not canon.** Point Worldbuilder at your map image and
it traces the coastline, meshes it, and works out heights, climate, rivers and biomes —
about 85 ms for a 2,000-pixel map. The inputs are your image and forty numbers in
`world.yaml`; the output is a cache you can delete. And because terrain does not change
with the date, the scrubber never pays for it.

**Contradictions are found deterministically, offline.** "Aldric died in 811 but attends
the Council of 814" is an interval containment test, not a job for a model. Six rules
cover existence violations, anachronisms, conflicting facts, orphan references,
succession gaps, and impossible parentage. Each finding says whether it is *definite* —
wrong under every reading of every fuzzy date — or merely *possible*, which is the shape
a deliberate mystery takes.

**AI is optional, and structurally so.** The app is fully functional with nothing
attached. A separate [MCP server](docs/mcp.md) exposes the world to whatever agent you
already use, and it **cannot write to your world** — every change lands in a review
queue that shows you what it would settle and what it would break, before you accept it.

## Try it

```sh
pnpm install
pnpm tauri dev
```

It opens `examples/vashen`: eleven records, three events, a border contested at a siege
dated only to the month, and one open question the consistency engine will not resolve
for you.

Jump to `@evt_siege_of_marrow` and watch the Vale go hatched. Then switch the terrain
layer to **rain** and see why the Vashen Empire wants the Vale at all. Then switch the terrain
layer to **rain** and look at why the Vashen Empire wants the Vale at all.

## Layout

| | |
|---|---|
| [`crates/wb-core`](crates/wb-core) | Calendars, fuzzy dates, anchor resolution, Allen's interval algebra |
| [`crates/wb-store`](crates/wb-store) | The file format, the loader, and the time-indexed queries |
| [`crates/wb-check`](crates/wb-check) | Deterministic consistency rules |
| [`crates/wb-terrain`](crates/wb-terrain) | The map pipeline — coastline, cells, height, climate, rivers, biomes |
| [`crates/wb-propose`](crates/wb-propose) | The review queue, and impact analysis |
| [`crates/wb-mcp`](crates/wb-mcp) | The agent surface — 17 tools, none of which reach canon |
| [`skills/`](skills) | Worldbuilding methodology, shipped separately from the app |
| [`src/`](src) | Svelte frontend: map, timeline, inspector, findings, queue |
| [`DESIGN.md`](DESIGN.md) | The design brief, and every decision that changed while building |

```sh
cargo test --workspace                      # 281 tests
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p worldbuilder --example check   # consistency report for the example world
cargo run --release -p wb-mcp --example scale   # what a 20,000-record world costs
cargo run --release -p worldbuilder --example terrain  # the map pipeline, plotted in ASCII
```

## Status

Slices 1–4 of six are complete: the temporal spine, the integrity layer, the agent
surface, and map depth. Next is story — scene stubs, external prose linking, and the
surfaced/iceberg view. See the [roadmap](DESIGN.md#11-roadmap).
