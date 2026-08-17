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

**Your files stay yours, down to the comments.** Editing a record in the app rewrites
only the lines that changed. The comment you left explaining a date, the inline style you
chose, the key order, a key this version has never heard of, and the prose below the
frontmatter all survive — so a one-word change is a one-line `git diff`, and the folder
does not slowly turn into machine output. When a file uses YAML the writer will not risk
touching, it says so before it writes anything, and you decide.

**The ground is derived, and it is not canon.** Point Worldbuilder at your map image and
it traces the coastline, meshes it, and works out heights, climate, rivers and biomes —
about 85 ms for a 2,000-pixel map. The inputs are your image and forty numbers in
`world.yaml`; the output is a cache you can delete. And because terrain does not change
with the date, the scrubber never pays for it.

**Contradictions are found deterministically, offline.** "Aldric died in 811 but attends
the Council of 814" is an interval containment test, not a job for a model. Seven rules
cover existence violations, anachronisms, conflicting facts, orphan references,
succession gaps, impossible parentage, and prose that names somebody who was not alive
when the scene is set. Each finding says whether it is *definite* — wrong under every
reading of every fuzzy date — or merely *possible*, which is the shape a deliberate
mystery takes.

**It can see how much of your world is actually on the page.** Point `manuscript.root` at
the folder your chapters live in, link a scene to a heading, and Worldbuilder reads the
prose — never writes it — and tells you which records surface and which do not. That is a
live [iceberg ratio](https://andreacerasoni.com/blog/iceberg-method), and the useful end
of it is not what you overbuilt but what the story keeps reaching for that was never
built. Every count shows the sentence it came from, because a number nobody can check is
a number nobody should act on.

**"What if" is a branch, and the diff is in records.** Your world is a folder of files, so
forking canon is `git branch` — and Worldbuilder reports the fork the way a writer thinks
about it: *"Aldric Vane — existence, and his death moves 810 days later; two open questions
settled, none introduced."* A record can appear in that list with nothing changed in its own
file and a date that moved anyway, because re-dating an event drags everything anchored to
it. No `git diff` can tell you that. Branching is only offered when the world folder is
itself the repository — a world nested inside a bigger one still gets its history and the
comparison, and is told plainly why the rest is off.

**Who followed whom, on the same axis as everything else.** A bloodline is people with
parentage edges and overlapping lifespans; a dynasty is a title passing along them. So the
lineage view is not a family tree — it is lifespans on a time axis, feathered where the
dates are guesses, with a held title drawn as a band, which makes a succession gap
literally a gap. It covers anything that changed hands: a duchy down three generations of
Vanes, or a valley passing between two empires at a siege.

**Hand it to someone as one file.** Export writes the whole world as a single
self-contained HTML page — the map, the timeline, every record cross-linked, no server and
no network. Three scopes: everything · *as it stood* on a date, which reads like a
gazetteer written that year · or only what your book has actually named, which is a
spoiler-free companion.

**AI is optional, and structurally so.** The app is fully functional with nothing
attached. A separate [MCP server](docs/mcp.md) exposes the world to whatever agent you
already use, and it **cannot write to your world** — every change lands in a review
queue that shows you what it would settle and what it would break, before you accept it.
Version control and publishing have no agent-facing tools at all: one rewrites your
repository and the other writes a file wherever you point it, and both are things a person
does.

## Try it

```sh
pnpm install
pnpm tauri dev
```

It opens the last world you had open, or `examples/vashen` on a first run: twelve records,
three events, a border contested at a siege dated only to the month, and two open questions
the consistency engine will not resolve for you.

Jump to `@evt_siege_of_marrow` and watch the Vale go hatched. Switch the terrain layer to
**rain** and see why the Vashen Empire wants the Vale at all. Switch the centre pane to
**lineage** and follow the ducal title down three generations of Vanes — one handover the
chronicle pins to the day, and one it does not, drawn differently because the difference is
the point. Then press **⤓ publish**, choose *as it stood*, and type `0810`: Marrow still has
nine thousand people in it.

## Layout

| | |
|---|---|
| [`crates/wb-core`](crates/wb-core) | Calendars, fuzzy dates, anchor resolution, Allen's interval algebra |
| [`crates/wb-store`](crates/wb-store) | The file format, the loader, the time-indexed queries, and the writer that puts records back without disturbing them |
| [`crates/wb-check`](crates/wb-check) | Deterministic consistency rules |
| [`crates/wb-terrain`](crates/wb-terrain) | The map pipeline — coastline, cells, height, climate, rivers, biomes |
| [`crates/wb-propose`](crates/wb-propose) | The review queue, and impact analysis |
| [`crates/wb-story`](crates/wb-story) | The manuscript read against the world — scenes, mentions, the iceberg |
| [`crates/wb-git`](crates/wb-git) | Save points, what-ifs, and reading an old revision back out |
| [`crates/wb-export`](crates/wb-export) | A world as one self-contained HTML document |
| [`crates/wb-mcp`](crates/wb-mcp) | The agent surface — 21 tools, none of which reach canon |
| [`skills/`](skills) | Worldbuilding methodology, shipped separately from the app |
| [`src/`](src) | Svelte frontend: map, lineage chart, timeline, inspector, findings, queue, record editor, story and version and export panels |
| [`DESIGN.md`](DESIGN.md) | The design brief, and every decision that changed while building |

```sh
cargo test --workspace                      # 457 tests
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p worldbuilder --example check   # consistency report for the example world
cargo run -p worldbuilder --example iceberg -- examples/vashen --mentions   # what reaches the page
cargo run -p worldbuilder --example export -- examples/vashen --at 0812 --out /tmp/bible.html
cargo run --release -p wb-mcp --example scale   # what a 20,000-record world costs
cargo run --release -p worldbuilder --example terrain  # the map pipeline, plotted in ASCII
```

## Status

All six slices are complete: the temporal spine, the integrity layer, the agent surface,
map depth, authoring, story, and depth. What is still open is open on purpose and named in
[§12](DESIGN.md#12-open-questions--risks) — polygon morphing, authoring shared borders, and
the half of the timeline-scale question a two-position toggle does not answer.
