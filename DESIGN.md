# Worldbuilder — Design Brief

A local-first worldbuilding tool where **time is a first-class axis** and **the map is a projection of the timeline**.

---

## 1. Thesis

Existing tools split into three camps, and nobody has merged them:

| Camp | Examples | Strength | Fatal gap |
|---|---|---|---|
| Structured wikis | World Anvil, Kanka, LegendKeeper, Campfire | Deep interlinking, rich articles | Map is pins on a static image; **time is just prose** |
| Temporal modelers | Aeon Timeline | Custom calendars, lifespans, arcs | **No map** |
| Time-varying atlases | Chronas, Running Reality, Omniatlas | Borders morph year-by-year | **Read-only, real world, not authoring** |

The empty middle: an **authoring** tool where the world's state is queryable at any date, and the map redraws itself as you scrub.

**The "oh" moment:** import a map image → draw two kingdoms → add an annexation event in 812 → drag the scrubber → watch the border move.

Everything else in this document amplifies that moment.

---

## 2. Decisions locked

| # | Decision | Choice |
|---|---|---|
| 1 | Primary user | **Novelists / fiction writers** |
| 2 | Map source | **Import image → vectorize → procedural assist → hand-draw places** |
| 3 | Time model | **Validity intervals, events as the authoring UI** |
| 4 | Date precision | **Fuzzy-first with relative anchoring** |
| 5 | AI | **Local MCP server, bring-your-own-agent, distributable skills** |
| 6 | Storage | **Plain files as truth**; derived indexes only where measurement calls for one (§11) |
| 7 | Manuscript | **Scene stubs + linked external prose (no editor)** |
| 8 | Ontology | **Small structural core, user-extensible types** |
| 9 | First slice | **Temporal spine end-to-end** |

---

## 3. Data model

### 3.1 Structural primitives

The engine reasons over five roles. User-facing type names ("Duchy", "Hive", "Orbital", "Ley Network") are labels declaring which primitive they behave like.

| Primitive | Has | Examples |
|---|---|---|
| **Actor** | existence interval, parentage edges, held titles | person, dragon, ship AI |
| **Polity** | territory over time, membership, rise/fall | kingdom, guild, corporation, church |
| **Place** | geometry, founding/destruction, changes hands | city, ruin, station, pass |
| **Event** | a date; asserts interval boundaries | battle, coronation, eruption |
| **Thing** | no geometry, may have intervals | language, magic system, artifact, tech |

Consequence: **lineage is not a special subsystem.** A bloodline is actors with parentage edges and overlapping existence intervals. A dynasty is a polity whose ruling-title interval passes along those edges. Same primitives, no bespoke genealogy engine.

### 3.2 Facts are time-indexed, not scalar

An entity's attributes carry validity intervals. Nothing is ever silently overwritten — new truth closes an interval and opens another.

```yaml
---
id: act_aldric_vane
type: noble              # user-defined
primitive: actor
name: Aldric Vane
existence: { from: "0771", to: "0811~" }
parents: [act_maren_vane, act_isolde_corr]
facts:
  - { attr: title,     value: "Duke of Corrath", from: "0799", to: "0811~" }
  - { attr: residence, value: place:corrath,     from: "0799", to: "0806"  }
  - { attr: allegiance, value: pol_corrath,      from: "0771", to: "0806"  }
  - { attr: allegiance, value: pol_vashen,       from: "0806", to: "0811~" }
---

Prose notes. Links to [[Corrath]] and [[Vashen Empire]].
```

### 3.3 Events as the authoring surface

An event is simply a dated occurrence. It carries **no `effects` block**.

```yaml
id: evt_siege_of_marrow
name: The Siege of Marrow
kind: conquest
date: "0812-04~"
participants: [pol_vashen, pol_corrath]
location: place_marrow
```

Facts reach the event by *anchoring to its date*, using the relative-anchor mechanism §3.4 already provides:

```yaml
# ter_vale_of_corrath
facts:
  - { attr: owner, value: pol_corrath, from: "@evt_founding", to: "@evt_siege_of_marrow" }
  - { attr: owner, value: pol_vashen,  from: "@evt_siege_of_marrow" }
```

> **Revised during implementation.** The original design had events mutating other records through an `effects` list. Building it showed that to be strictly worse: two sources of truth for the same interval, order-dependent files, and an effect-replay engine to write and debug. Anchoring inverts the dependency — facts own their own validity and *reference* events — which costs nothing, because re-dating an event already drags every anchored fact with it.
>
> "Events as the authoring surface" survives as a **UI affordance**: an "add annexation" command writes the two fact edits. The file format stays declarative and order-independent.

### 3.4 Fuzzy dates

A date is **not a scalar** — it is a constrained interval with a nominal rendering point and an optional relative anchor.

```
Date := {
  earliest: Instant | null
  latest:   Instant | null
  nominal:  Instant | null            # best guess, used for rendering
  anchor:   { ref: EntityId, offset: Duration } | null
}
```

Proposed input notation:

| Input | Meaning |
|---|---|
| `812` | exact |
| `812~` | approximately |
| `810..815` | somewhere in range |
| `>812` / `<812` | after / before |
| `@evt_sundering+40y` | relative anchor |
| `@act_aldric.birth+2g` | anchored to another entity's boundary, in generations |

**Relative anchoring is the killer feature.** Dates form a DAG; resolution is a topological sort with interval constraint propagation. Move the Sundering and everything pinned to it moves. Novelists re-time their history constantly, and every other tool makes that a manual rewrite.

Formal grounding: **Allen's interval algebra** (the 13 relations — before, meets, overlaps, during, starts, finishes, equals, and inverses) is the right foundation for both the constraint solver and the consistency rules.

Uncertainty must **render**: soft/gradient edges on the timeline, dashed or hatched borders on the map. Vagueness is information, not an error state.

### 3.5 Calendar engine

Table stakes — it is the single most-cited Aeon Timeline feature. Arbitrary month names and lengths, weekday counts, era systems with offsets, leap rules, optional multiple moons. All dates are stored as a canonical scalar (days-since-epoch) and *rendered* through the calendar, so calendar edits never corrupt data.

### 3.6 Territory over time

A territory is a set of `(polity, geometry, interval)` triples. Rendering at time *T* selects features whose interval contains *T*.

Two hard problems, flagged early:

- **Shared borders.** If two kingdoms share a frontier, moving it must not require editing two independent polygons. Use a **topological model** (shared arcs, TopoJSON-style) rather than free-floating polygons.
- **Morphing.** Tweening polygons with different vertex counts is genuinely hard. Cross-fade is the pragmatic default for v1; true morph requires vertex correspondence. Do not let this block the slice.

**Scrub performance trick:** precompute the sorted set of all interval endpoints ("change points"). Scrubbing *between* change points requires no requery — only crossing one triggers a diff. This is what makes the scrubber feel buttery instead of laggy.

---

## 4. Map pipeline

Every stage optional, every stage overridable. The imported raster stays as a display layer; the vectors are what's queryable.

1. **Import raster** — any map image (Inkarnate, Wonderdraft, commissioned art, phone photo of a napkin sketch).
2. **Land/sea segmentation** — user picks the sea color, or brush-assisted (GrabCut-style). A manual touch-up brush is non-negotiable; auto-segmentation will always be wrong somewhere.
3. **Contour trace** — marching squares → raw polygon.
4. **Simplify** — Douglas–Peucker or Visvalingam, exposed as a user-facing "detail" slider.
5. **Cell substrate** *(optional)* — Poisson-disc sampling → Delaunay/Voronoi cells over land. This is the grid Azgaar-style generators compute on.
6. **Heightmap** — painted by hand or generated with coastal falloff.
7. **Climate** — latitude bands + prevailing winds + orographic rainfall (rain shadow behind ranges).
8. **Rivers** — flow accumulation downhill across cells, merge into streams, carve channels.
9. **Biomes** — temperature × precipitation via a Whittaker-style lookup.
10. **Hand-authoring** — cities, borders, routes, regions drawn on top. This is where the human's world actually lives.

### Built in slice 4 — `wb-terrain`

Stages 2–9 all shipped, as eight pure functions in one crate that knows nothing about worlds, entities or dates. The whole pipeline is 85 ms on a 2,000 × 1,400 raster at 2,800 cells.

**Terrain is a build product, not canon.** This is the decision the rest follows from. The *inputs* are a map image and about forty numbers in `world.yaml` — small, versioned, the writer's own. The *output* is a few thousand cells, cached in `.worldbuilder/` under a key that mixes the parameters' digest with the image's own bytes, and never committed. Delete it and it rebuilds; change a slider and the key moves.

That split has a second consequence that matters more than the caching: **terrain does not change with the date.** So the political layer is refetched every time the scrubber crosses a change point, and terrain is fetched once per world and never again. The map is still a projection of the timeline; the ground it is projected onto is not.

Three things worth recording:

**A negative `peak` is a valley.** Ranges are authored as a line with a width and a height — `{ from, to, peak, width }`. Nothing needed adding to carve a river valley through a mountain gap: it is the same primitive with the sign flipped. The Vashen example uses both, and the Silt's course through the Marrow Wall is authored rather than hoped for.

**The rain shadow does real work.** Rainfall is a single upwind-to-downwind sweep, valid because projecting the cells onto the wind vector gives a total order. In the example a westerly comes off the sea, drops 0.49 of the map's maximum on Corrath in the Vale, 0.24 on Marrow, and 0.08 on Hold Vashen behind the wall. The Vale is the productive part and the Vashen heartland is dry steppe — which is a reason for an empire to want somebody else's valley, and it came out of the physics rather than being written into a fact.

**Segmentation has to survive a real export.** The example raster ships with antialiased coasts and fourteen hundred wrong-colour specks, because that is what an export looks like. `min_blob_px` eats them; without it the world acquires three hundred one-pixel islands.

Stage 10 was already in place from slice 1: markers and polygons authored inline on the entity, in the same normalized coordinate space the pipeline works in, which is what lets the inspector say what the ground under a settlement is like without a second query.

---

## 5. Consistency engine

**Most consistency checking is not AI.** These are deterministic rules over the interval model — instant, free, offline, and incapable of hallucinating. Built natively in `wb-check`.

### Every finding carries a certainty

This is the part that makes the engine usable rather than annoying:

- **Definite** — wrong under *every* reading of every fuzzy date. Fix it.
- **Possible** — the world's own vagueness leaves room. A deliberate mystery has exactly this shape, so it is never presented as an error.

The seed world reports `0 definite, 1 possible`: the Marrow chronicle puts Aldric Vane at the walls a year after his recorded death of `0811~`. The engine says both readings are live and declines to pick. **That is the case a rigid checker gets wrong** — it would either stay silent or cry error, and both are useless to a writer.

AI's job starts only where judgment does: *"is this a contradiction, or a deliberate mystery?"* Detection needs no model at all.

### The rules

| Rule | Detects | Certainty |
|---|---|---|
| `existence-violation` | An event names a participant or location that did not exist then | both |
| `anachronistic-fact` | A fact points at something that did not exist while it held | both |
| `conflicting-facts` | One attribute asserted two ways over days where both are settled | definite |
| `orphan-reference` | A reference to an id nothing defines | definite |
| `succession-gap` | A single-valued attribute with a stretch nothing covers | definite |
| `impossible-parentage` | A child born before their parent, or beyond death + gestation | both |

Three refinements the implementation forced, each of which prevents a class of false positive:

- **Vague overlaps are never conflicts.** Only *certain* intervals overlapping is a contradiction. Two claims overlapping in their fuzzy edges is the contested-border case — the thing the map exists to show — and flagging it would turn the feature into an error.
- **Entities bounded by an event are exempt from it.** A duchy annexed at the siege takes part in the siege; a city founded by a founding hosts it. Their lifespans end or begin exactly there, so the boundary always looks like a near-miss.
- **Gaps are measured on possible intervals, not certain ones.** Two facts meeting at a vague event leave a hole between their certain cores. That hole is uncertainty, not an unruled decade.

Two checks from the original list live earlier in the pipeline and are deliberately absent: **anchor cycles** and **impossible calendar dates** both fail at load, because a world that cannot resolve its own dates cannot be queried at all. **Scene contradictions** wait for Slice 5, when scenes exist.

Per-world knobs live in `world.yaml` under `rules`: `multi_valued` (attributes that may legitimately hold several values, so `member` is not a conflict) and `gestation_days`.

```bash
cargo run -p worldbuilder --example check          # exits non-zero on definite findings only
```

---

## 6. Storage & version control

Plain files the user owns. SQLite is a rebuildable derived index, gitignored.

```
my-world/
├── world.yaml                       # meta + calendar definition
├── entities/
│   ├── actors/aldric-vane.md
│   ├── polities/vashen-empire.md
│   ├── places/corrath.md
│   └── things/high-tongue.md
├── events/
│   └── 0812-siege-of-marrow.yaml
├── geometry/
│   ├── base.topojson                # coastline, terrain
│   └── territories/*.topojson       # feature per validity interval
├── scenes/
│   └── ch12-s03.yaml
├── proposals/                       # agent output awaiting review
└── .worldbuilder/
    └── index.sqlite                 # derived, gitignored
```

Markdown + frontmatter for prose-bearing entities; YAML for pure structure; TopoJSON for geometry.

**Free bonus: git branching gives you "what if" histories.** Fork canon, redraw two centuries of borders differently, diff the worlds, merge or discard. No tool in this space has this — it falls straight out of the storage decision.

At realistic scale (~10k entities, ~50k intervals) a full reindex is well under a second, so file-as-truth costs nothing in practice.

---

## 7. MCP server & bring-your-own-agent

The app exposes the world through a **local MCP server**. The user brings their own agent — Claude Desktop, Claude Code, Cursor. No API keys in the app, no vendor lock-in, no inference cost on you.

This makes "AI isn't the main driver" **structurally true**, not just a design promise: the app is fully functional with zero agents attached. AI is an optional *client* of the data model, never a layer inside it.

### 7.1 Tool surface

Built as `wb-mcp` on `rmcp` 3.1, served over stdio. Setup: [`docs/mcp.md`](docs/mcp.md).

**Read (safe, unrestricted):**
- `describe_world()` — calendar, date syntax, type and attribute vocabulary, consistency standing
- `world_at(date)` — snapshot summary
- `get_entity(id, at?)` — facts resolved at a date
- `query_entities(filter)` · `timeline(from, to, filter)` · `search(text)`
- `territory_at(date)` — GeoJSON-shaped
- `lineage(actor_id, depth)` · `check_consistency(scope?)`
- `resolve_date(expr)` — check an anchor before writing it
- `list_notes()` / `read_note(path)` — the writer's raw source material

**Write (routed to a proposal queue, never direct to canon):**
- `check_changes(changes)` — dry run: what it settles, what it breaks. Writes nothing
- `propose_changes(title, note, changes)` — files one proposal
- `list_proposals(status?)` — human accepts or rejects in-app

Agent writes landing straight in canon is how a novelist loses trust in one bad session. The proposal layer costs nothing extra — writers want canon-vs-speculative staging anyway. **There is no accept tool, and no way to add one from the agent side**; a test asserts that `propose_changes` is the only tool on the surface not marked read-only.

Four substitutions against the sketch above, all made while building:

**`describe_world` is the highest-leverage tool on the surface** and was not in the original list. An agent that does not know the months are thirty days long, that `~` widens a year by two here, or that this world already calls a ruler `owner`, produces dates that resolve to the wrong day and facts that fork an existing vocabulary under a new name. None of that is recoverable downstream. It carries the calendar, the fuzz constants, a date-syntax table, declared types with usage counts, and every attribute already in use with example values.

**One `propose_changes` rather than `propose_entity` / `propose_event` / `propose_fact`.** A proposal is a unit of *review*, not a unit of edit. Ingesting a chapter of notes as forty proposals is forty decisions, and a writer will reject the lot rather than work through them. One tool taking a list mirrors the file format exactly and keeps a coherent idea together.

**`check_changes` — a dry run — was added** so an agent can check its own work before spending a writer's attention. It reports the same impact analysis as the queue and writes nothing. A proposal that cannot even be simulated is refused rather than filed: that is a bug report, not a suggestion.

**The change vocabulary is mirrored, not reused.** `ChangeInput` restates `wb_propose::Change` with dates as plain strings, parsed at the boundary. A malformed date then produces ``bad `date` "0812~~": unexpected '~'`` instead of a serde error naming a field, and the schema stays shallow — nested optional structs are exactly where a model drops a level.

**`territory_at` returns GeoJSON's shape but says it is not geographic.** Coordinates are normalized 0–1 image space with the origin top-left, so `y` increases *southward*. Every response carries `coordinate_space` and a note saying so, rather than letting a consumer discover the flip by rendering a mirrored world.

### 7.2 The proposal layer

Built as `wb-propose`, and **independent of MCP** — it is the integrity mechanism that makes an agent surface safe, not a part of the agent surface itself.

A proposal is a YAML file in `proposals/` holding granular changes rather than a whole-file replacement, so a reviewer reads what is being asked for instead of diffing YAML in their head, and nothing unrelated can be smuggled into a blob:

```yaml
id: prp_resolve_aldric
title: Aldric died of his wounds within the year
author: claude-desktop
note: >
  The chronicle puts him at the walls in 812, but his death is recorded as 0811~.
  If the chronicle is right, the recorded death is what is wrong.
status: pending
changes:
  - op: set_existence
    entity: act_aldric_vane
    to: "@evt_siege_of_marrow+1y"
```

Six operations cover the realistic surface: `create_entity`, `create_event`, `add_fact`, `remove_fact`, `set_existence`, `set_event_date`.

**The queue is worth more than a list of pending edits because of impact analysis.** Before accepting anything, `wb-propose` simulates the change in memory, re-runs the consistency engine, and diffs the findings — so the writer sees which contradictions a proposal settles and which it creates. The example world ships one of each:

| Proposal | Effect |
|---|---|
| *Aldric died of his wounds within the year* | settles 1 · breaks nothing |
| *Vashen ruled from Marrow before the conquest* | settles 0 · **adds 2 definite** |

The second reads perfectly plausibly and is wrong twice over — Marrow does not exist until `0602~`, and Vashen already has a capital across those years. It is caught before it lands, not after.

Findings are matched on what they are *about* (rule + subject + related), never on their wording — a proposal that shifts a date would otherwise look like it had resolved a problem and introduced an identical one.

**Writing is deliberately conservative.** Rendering happens in full before anything is written, so a proposal that cannot be applied completely is not applied in part. Prose bodies are preserved verbatim. Decided proposals stay on disk carrying their status — provenance that matters more once agents are filling the queue.

**Two stated limitations.** Frontmatter is rewritten canonically, so comments inside it do not survive and a one-line change can produce a wide diff; the diff is shown before accepting and every write is an ordinary git change. And a key the model does not understand is *refused* rather than dropped — `Error::WouldDropKey` blocks the write entirely, because this tool holds people's life's work.

### 7.3 Shippable skills

Skills are where worldbuilding *methodology* lives, distributable and extensible without shipping app updates. All six ship in [`skills/`](skills/):

- `consistency-audit` — reason over deterministic violations, separate bugs from mysteries
- `world-from-notes` — ingest an existing 40-page doc into structured entities (**the single biggest adoption barrier in this space is re-entering years of notes by hand**)
- `chapter-canon-check` — read a linked scene, check it against world state at that date
- `culture-from-phonology` — names consistent with an existing language
- `succession-crisis` — plausible dynastic fallout
- `iceberg-check` — what have you overbuilt that never surfaces in the manuscript?

The tools say what an agent *can* do; the skills say what is worth doing, and — more often — what to refuse on the writer's behalf. Every one ends the same way: **offer, do not decide.** A name is taste, a mystery is a plot, and where to spend the next hour is the writer's call.

The methodology worth naming here is `world-from-notes`'s: **preserve the vagueness.** Notes are vague because worlds are vague, and the instinct to tidy that away is the failure mode. "Roughly 600 AR" becomes `0600~`, never `0600`; "after the Sundering" becomes `>@evt_sundering`, never an invented year. Every piece of precision an agent adds is a fact the writer never wrote and will later have to discover is wrong.

`crates/wb-mcp/tests/skills.rs` asserts that every tool the server exposes is named by at least one shipped skill, so a tool never ships without methodology attached. It has already caught one: `list_proposals` had no skill telling an agent to check the queue before adding to it.

### 7.4 Staying honest against the disk

The server is long-lived and the writer is editing these same files in their own editor while it runs — that is the point of files-as-source-of-truth. A server that loaded once at startup would confidently answer with yesterday's canon.

So every call fingerprints the world tree (path, size, mtime) and reloads when anything moved. The fingerprint is a walk; the reload is a walk plus a parse, and skipping the parse is most of the win. A reload that fails — a half-saved file mid-keystroke — is not fatal: the last good world keeps answering and the error says so rather than a stale answer passing as fresh.

---

## 8. Manuscript integration

The app **never owns or edits prose.** Writers are attached to Scrivener, Obsidian, and Word, and will not move.

A scene is a first-class entity — which fits the model perfectly, since **a scene *is* an interval with a POV character and a location**:

```yaml
id: scn_ch12_s03
primitive: event
date: "0812-04-17"
pov: act_aldric_vane
location: place_marrow
source: ../manuscript/ch12.md#scene-3     # read-only link
```

This gives you, with zero editor built:
- The story rendered on the timeline against world history
- The book's path lit up on the map
- Prose checkable against canon
- The agent seeing both world and manuscript

**Derived feature worth calling out:** scanning linked prose for entity mentions yields a **"surfaced" flag** — which parts of the world actually appear on the page. That's a live [iceberg ratio](https://andreacerasoni.com/blog/iceberg-method): the 10% above water vs the 90% below. No other tool can show a writer that.

---

## 9. Worldbuilding coverage

From the research, the domains templates and skills should cover — offered as *prompts, never required fields*:

- **Physical** — geography, climate, seasons, celestial bodies, resources
- **Peoples** — cultures, customs, food, dress, taboos, class
- **Language** — naming conventions, phonology, scripts
- **Belief** — religion, cosmology, myth, afterlife, clergy
- **Power** — polities, succession law, military, diplomacy
- **Economy** — trade goods, routes, currency, guilds
- **Knowledge** — technology level, magic system *with hard rules and costs*, medicine, education
- **History** — eras, founding myths, wars, migrations, plagues, collapses

Two methodology constraints on the UX:

**Never force top-down.** Most tools assume planet → continents → civilizations → regions and induce *worldbuilder's block*. Support bottom-up growth equally: start with one tavern and expand outward as the story demands. The interval model is naturally agnostic — an entity with no dates and no geometry is perfectly valid and can be filled in later.

**Respect the iceberg.** ~90% of a world never reaches the page. The tool must comfortably hold far more than it shows and never imply that empty fields are incomplete work.

---

## 10. Proposed stack

| Layer | Choice | Note |
|---|---|---|
| Shell | Tauri v2 | Known quantity |
| Backend | Rust | |
| Frontend | TypeScript + React or Svelte | |
| Map render | Hand-rolled SVG | Still holds at 2,800 terrain cells — see §11. Swap to WebGL if it stops holding |
| Geometry (Rust) | ~~`geo`, `geo-types`~~ · `delaunator` | **Substituted.** Only Delaunay was worth a dependency; simplify, clip and point-in-polygon are a page each and had to be bit-for-bit stable |
| Geometry (TS) | ~~`turf.js`, `topojson`~~ | **Not needed.** The frontend receives finished geometry and draws it; it computes none |
| ~~Index~~ | ~~`rusqlite` + R\*Tree~~ | **Dropped.** Measured: queries are linear and sub-10 ms at 20,000 records. See §11 |
| Vectorization | `image` (PNG only) + marching squares | Marching squares hand-rolled: the saddle cases have to be resolved the same way every time, and that is the whole algorithm |
| MCP | `rmcp` 3.1 (official Rust SDK) | Sidecar binary over stdio — the app need not be running |
| Randomness | hand-rolled SplitMix64 | Terrain is a cache key, and `rand` may change its algorithms between versions |
| Git | `git2` (libgit2) | Branching histories |

---

## 11. Roadmap

| Slice | Contents |
|---|---|
| **1 — Temporal spine** ✅ | Interval model, file storage, timeline scrubber, map with hand-drawn vector regions, fuzzy dates + relative anchoring, calendar engine |
| **2 — Integrity** ✅ | Deterministic consistency engine ✅ · extensible ontology ✅ · proposal/draft layer ✅ |
| **3 — Agent surface** ✅ | MCP server ✅ · tool surface ✅ · shipped skills ✅ · notes ingestion ✅ |
| **4 — Map depth** ✅ | Coastline vectorization ✅ · cell substrate ✅ · heightmap ✅ · climate ✅ · rivers ✅ · biomes ✅ |
| 5 — Story | Scene stubs, external prose linking, surfaced/iceberg view |
| 6 — Depth | Git branching UI, lineage/dynasty views, export & publish |

Slice 1 is deliberately the shortest path to the moment that proves the thesis — and it forces the hardest decisions (interval semantics, fuzzy date resolution, scrub performance) while the codebase is still small enough to throw away.

### Status

| Crate / step | State |
|---|---|
| `wb-core` — calendars, fuzzy dates, anchor resolution, Allen intervals | **done**, 56 tests |
| `wb-store` — file format, loader, world assembly, time-indexed queries, search | **done**, 23 tests |
| `wb-check` — six deterministic consistency rules | **done**, 17 tests |
| `wb-propose` — review queue, impact analysis, applier | **done**, 16 tests |
| `wb-mcp` — MCP server, 17 tools, notes ingestion, terrain queries | **done**, 38 tests |
| `wb-terrain` — the eight-stage map pipeline | **done**, 123 tests |
| `skills/` — six shipped methodologies | **done** |
| `examples/vashen` — a working seed world | **done**, 11 entities, 3 events, 2 proposals, 1 notes file, 1 map |
| Tauri commands — query surface for the frontend | **done**, 8 payload tests |
| Svelte map with five terrain layers, timeline, inspector, findings, review queue | **done** |
| SQLite index | **not needed** — see below |

**281 tests** across the workspace. Clippy clean under `-D warnings`; `svelte-check` reports 0 errors and 0 warnings.

Slices 1 through 4 are complete. **Slice 5 (story) is next.**

§12.7 was aimed squarely at slice 4 — the map pipeline is the rabbit hole that can swallow months. What kept it to a day was one rule held to throughout: **every stage is a pure function, and the whole thing is a build product.** No stage may consult a world, a date or an entity; no output is ever committed. That made the eight stages independently implementable and independently testable, and it made the tuning loop an ASCII plot in a terminal rather than a round trip through the UI.

The part that did burn time was not the algorithms. It was the *parameters*: the first run produced a map that was 29% cold desert with a town of nine thousand sitting in one, and 63 rivers. Every stage was correct. Nothing but the dials was wrong — which is the strongest argument for keeping them in the writer's own `world.yaml` where they can be argued with.

Two Slice-1 substitutions, both reversible:

**Geometry lives inline on the entity**, as normalized 0–1 coordinates (`marker`, `shape`), not in separate TopoJSON files. Normalized rather than pixels so a backdrop can be swapped or redrawn without moving a settlement. TopoJSON earns its place when shared-border editing arrives (§3.6) — until then it is a file format standing in for a dozen polygons.

**The map is hand-rolled SVG, not Leaflet.** Leaflet's value is tiled raster handling, which is a Slice 4 concern; today it would be machinery wrapped around a handful of shapes. Hand-rolled SVG also gives exact control over the thing that matters most here — rendering an *uncertain* border as hatched and dashed rather than picking a side. Screen-to-world conversion uses the SVG's own `getScreenCTM()`, so it stays correct under any aspect ratio.

### Verified end to end

The thesis moment works in the running app. Jumping to the siege renders the Vale hatched and dashed with both claims live, Aldric Vane flagged uncertain, and the Duchy of Corrath uncertain alongside it — no verdict invented anywhere.

**The change-point claim measured in the live UI: 121 scrub steps produced 3 snapshot queries.** Scrubbing 35 years cost one additional fetch. The header carries a `queries / scrub steps` readout so the ratio stays visible while developing rather than being an assumption.

UI verification runs through [tauri-pilot](https://github.com/mpiton/tauri-pilot), registered under `#[cfg(debug_assertions)]` so it never ships. Three setup notes worth keeping:

- `"pilot:default"` must be in `capabilities/default.json`. Without it `ping` succeeds but every JS command fails with an opaque `eval timed out after 10s`.
- Launch with `pnpm tauri dev`, not `cargo run` — a debug build points the webview at `devUrl`, so without Vite the page is blank and there is no bridge.
- `tauri-pilot windows` panics the app inside `wry` (`wkwebview/mod.rs:1349`). Every other command is fine.

`cargo run -p worldbuilder --example dump -- <date>` prints the exact payload the UI receives, for any date expression including anchors.

**Slice 3 verified across both halves of the system.** A scripted MCP session — the real binary, real stdio, real JSON-RPC — read `notes/houses-and-holdings.md`, confirmed House Ferrow was absent from canon (`search("ferrow")` → 0 hits), dry-ran three `create_entity` changes, and filed one proposal. Then, in the running app, the header moved to **3 pending** and the queue showed the new proposal beside the two shipped ones:

| Proposal | Effect |
|---|---|
| *Aldric died of his wounds within the year* | settles 1 |
| *Vashen ruled from Marrow before the conquest* | **adds 2** |
| *House Ferrow holds Greyford* (filed over MCP) | no effect on checks |

Its detail panel carried the author byline, the note citing the source file, the three changes in plain language, and the file diff — with `existence.to: '?'` intact, which is the thing that matters. An agent that had tidied that into a year would have invented canon, and the writer would have had no way to see it happen.

The example world was restored afterwards, and the notes deliberately left **un-ingested**: `world-from-notes` should have real work to do the first time someone runs it.

### Slice 4 verified in the running app

The five terrain layers, the imported raster underneath them, and the political layer on top — all in the shipped example, driven through [tauri-pilot](https://github.com/mpiton/tauri-pilot).

| What it shows | What it proves |
|---|---|
| **Biome** — forest on the west coast, grassland through the middle, cold desert in the east | The Whittaker table and the climate feeding it agree with each other |
| **Rain** — teal along the west and south coasts, sand in the eastern interior | The rain shadow. One picture, and the Vashen Empire has a motive |
| **Height** — the Marrow Wall in two pieces with a gap at Marrow's latitude, the Silt running through it | Authored relief, including the negative-peak valley, lands where it was drawn |
| **Imported map** — the writer's own raster, with the traced coastline stroked over it | §4's claim: the raster stays as a display layer, the vectors are what is queryable |

Selecting Corrath reports its ground without a second query: *temperate forest · on a river · 9.8 °C · 49% of the wettest*. Terrain does not change with the date, so the join is computed once alongside the terrain itself.

Three bugs the work turned up, all found by looking at the map rather than by a test:

- **The first raster was too smooth to vectorize.** Its fBm scaled both the lattice size and the coordinates by the octave, so every octave sat on top of the others and the coastline had detail at exactly one scale. The detail slider had nothing to remove.
- **All sea was "shallows".** The shelf falloff is broad by design on a map that is mostly interior, and the same parameter governs both sides of the shore — so no sea cell was ever far enough from land to be deep. Left as it is: on a regional map, a shelf sea is the correct answer.
- **The Silt pooled into a lake short of the coast.** The authored valley stopped inland of the shore, and the flood filled the gap. Extending it to the firth fixed it — but the mere left in its upper reach is real: the interior there is flat, because the western and southern coasts are equidistant and the falloff has no gradient to give.

### On SQLite: measured, and dropped

Slice 1 deferred the index with a promise to revisit once there were real query shapes to index *for*. Slice 3 produced them, so it was measured rather than argued about. `cargo run --release -p wb-mcp --example scale` generates worlds shaped like real ones — places changing hands at events, actors with parentage, fuzzy lifespans — and times what the server actually pays:

| entities | events | load | fingerprint | `world_at` | `search` | `check_consistency` |
|---:|---:|---:|---:|---:|---:|---:|
| 200 | 20 | 18 ms | 0.6 ms | 0.03 ms | 0.12 ms | 0.4 ms |
| 1,000 | 100 | 47 ms | 2.2 ms | 0.17 ms | 0.42 ms | 1.9 ms |
| 5,000 | 500 | 261 ms | 12.8 ms | 1.09 ms | 2.14 ms | 10.2 ms |
| 20,000 | 2,000 | 1,022 ms | 64.6 ms | 5.15 ms | 8.42 ms | 45.0 ms |

A working novelist's world is a few hundred records; a decade of World Anvil material reaches a few thousand. 20,000 is past anything anyone has.

**The queries were never the problem.** Every one of them is linear and lands in single-digit milliseconds at a scale nobody will reach. An index makes an already-imperceptible query imperceptible, at the cost of a schema, a migration story, and a second source of truth to fall out of sync with the files. That is the trade SQLite was going to buy, and it is not worth making.

The two costs that *are* real are both structural, and neither is a query:

- **Parsing at launch**, at roughly 50 µs per record. One second at 20,000 records, and that is a cold start, once.
- **The freshness walk**, at roughly 3 µs per file on every call. Measuring it paid for itself immediately: the walk was taking metadata from the `Path` rather than the `DirEntry` the OS had already filled in, which is two extra syscalls per file. Fixing that halved it.

If either ever bites, the answer is still not SQLite. Launch parsing wants a cached parse keyed on the fingerprint; the freshness walk wants a filesystem watcher, which turns a per-call cost into zero. **The trigger to revisit is a query going superlinear or a working set exceeding memory — not a record count.** Neither is close.

---

## 12. Open questions & risks

1. **Polygon morphing** — cross-fade v1, but true border morphing needs vertex correspondence. Unsolved.
2. **Shared-border topology** — TopoJSON arcs are the right call, but authoring UX for shared edges is genuinely hard.
3. **Scrub performance** — change-point precomputation holds, and the terrain layer costs nothing extra because it is fetched once and never refetched. Still unvalidated at thousands of *time-varying* features.
4. **Timeline scale range** — 4,000 years of history and a six-week story on one axis. Likely needs multi-resolution zoom (eras → centuries → years → days), a story-window bookmark, and an event-density minimap.
5. **Constraint solver complexity** — fuzzy anchors form a DAG needing cycle detection and interval propagation. Keep it simple; resist building a general temporal reasoner.
6. **Blank page** — needs seed worlds and templates, or slice 1 demos to an empty screen.
7. **Scope discipline** — every section above is a product on its own. ~~The map pipeline in particular is a rabbit hole that can swallow months.~~ **Survived** (§11): pure stages, a build-product output, and an ASCII plot to tune against. The rabbit hole turned out to be the parameters, not the code.

---

## Sources

- [Kanka vs World Anvil comparison](https://kanka.io/kanka-vs-worldanvil)
- [World Anvil vs LegendKeeper vs Kanka](https://stormscape.app/blog/world-anvil-vs-legendkeeper-vs-kanka-vs-stormscape)
- [Best worldbuilding tools 2026](https://storyflow.so/blog/best-tools-worldbuilding-2026)
- [Aeon Timeline — worldbuilding](https://www.aeontimeline.com/solutions/worldbuilding) · [custom fantasy calendars](https://help.aeontimeline.com/article/60-custom-fantasy-calendars)
- [Omniatlas](https://omniatlas.com/) · [Timelory historical atlas](https://timelory.com/history/historical-atlas.html) · [Mapping History roundup](https://googlemapsmania.blogspot.com/2021/12/mapping-history.html)
- [Worldbuilding essentials — top-down vs bottom-up](https://noveling.dev/guide/en/blog/worldbuilding-essentials/)
- [The Iceberg Method](https://andreacerasoni.com/blog/iceberg-method)
- [Leaflet CRS.Simple non-geographic maps](https://leafletjs.com/examples/crs-simple/crs-simple.html)
