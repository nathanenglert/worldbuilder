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
| 7 | Manuscript | **Scene stubs + linked external prose (no editor)** ✅ |
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

**Slice 6 had to honour that claim rather than quietly abandon it, and it holds.** `wb-store::kin` is two walks over `parents:` and one grouping over facts that already exist: no family-tree type, no marriage edge, no house record. The lineage view is rows of existence intervals with title tenures drawn as bands on them, which is what the sentence above describes if you take it literally.

The one thing that *was* missing is a transpose, not a subsystem. `succession-gap` runs per entity per attribute — "was Aldric ever not the duke". A dynasty asks the other way round: group every fact by `(attr, value)`, and who held **the** title falls out. It turns out there are two shapes of baton and they render identically — a **title** is one value with many holders (`title: "Duke of Corrath"`, held by three Vanes), an **office** is one record's attribute with many values (the Vale's `owner`, held by a duchy and then an empire). Restricting the query to actors and titles would have been exactly the bespoke genealogy engine this section rules out.

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
| `scene-contradiction` | A scene's prose names someone who was not alive when it is set | both |

Three refinements the implementation forced, each of which prevents a class of false positive:

- **Vague overlaps are never conflicts.** Only *certain* intervals overlapping is a contradiction. Two claims overlapping in their fuzzy edges is the contested-border case — the thing the map exists to show — and flagging it would turn the feature into an error.
- **Entities bounded by an event are exempt from it.** A duchy annexed at the siege takes part in the siege; a city founded by a founding hosts it. Their lifespans end or begin exactly there, so the boundary always looks like a near-miss.
- **Gaps are measured on possible intervals, not certain ones.** Two facts meeting at a vague event leave a hole between their certain cores. That hole is uncertainty, not an unruled decade.

Two checks from the original list live earlier in the pipeline and are deliberately absent: **anchor cycles** and **impossible calendar dates** both fail at load, because a world that cannot resolve its own dates cannot be queried at all.

**Scene contradictions arrived in slice 5**, and are the one rule whose body does not live in `wb-check`. It needs to open a file the world does not own, and `wb-check`'s whole claim is that it is interval arithmetic over facts the writer already stated — instant, offline, and incapable of inventing a contradiction that is not there. So `wb-check` owns the rule's *name* and `wb-story::canon` owns its body, and both produce the same `Finding`. One `wb_story::check` merges them, which is what every caller uses: the app's header count, `check_consistency`, the impact analysis behind every proposal, and `--example check`. Two surfaces disagreeing about whether a world is clean would be worse than either answer.

Scenes also join `existence-violation` and `orphan-reference` for free, because a scene is a dated record naming records — the same shape those two rules already understood.

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
├── scenes/                          # where the telling touches the world
│   └── the-breach.yaml
├── proposals/                       # agent output awaiting review
└── .worldbuilder/
    └── index.sqlite                 # derived, gitignored

../manuscript/                       # outside the folder, and never written to
└── ch12-the-siege.md
```

Markdown + frontmatter for prose-bearing entities; YAML for pure structure; TopoJSON for geometry.

**Free bonus: git branching gives you "what if" histories.** Fork canon, redraw two centuries of borders differently, diff the worlds, merge or discard. No tool in this space has this — it falls straight out of the storage decision.

**Built in slice 6, and it did not fall out quite as straight as that.** Three things had to be decided before any of it was safe:

**A world folder is very often not a repository.** `Repository::discover` from `examples/vashen` returns the Worldbuilder source tree, so a "try a what-if" button wired to it would branch and check out this codebase — and a vault inside a dotfiles repo, or a world in a monorepo, is the ordinary shape for the people this is for. So `wb-git` reports a three-way `Standing` and gates every mutation on the world folder *being* the repository root. A **nested** world is not crippled: reading history and materializing an old revision touch nothing, so the whole comparison feature stays available, with one sentence naming the repository the buttons would have moved.

**A branch is reported in records, not in lines.** That is the part worth having. `git diff` can say eleven lines of `aldric-vane.md` changed; `wb_propose::diff_worlds` says *"Aldric Vane — existence, and his death moves 810 days later; two open questions settled, none introduced."* A record can also appear with **no changed fields and a moved date**, because re-dating an event drags every fact anchored to it — the consequence a line diff structurally cannot show.

**The other side of a comparison has to be a world, not a directory of files.** Materializing a revision into a scratch directory leaves `manuscript.root` — `../manuscript`, relative to the world folder — pointing at nothing, so `wb_story::check` finds no prose on that side and the panel reports the branch as *settling two contradictions it never touched*. The scratch directory therefore lives in `.worldbuilder/`, whose leading dot `freshness::walk` skips (materializing anywhere else inside the folder would reload the world underneath itself), it is removed by an RAII guard, it is excluded from status and staging — otherwise one comparison would leave untracked files and the next branch switch would be refused because of them — and the manuscript root is overridden in memory to the live one. That last is not a workaround: the book is deliberately outside the repository (§8) and is not versioned with the world, so comparing two revisions against *today's* manuscript is the only honest reading.

Four things are refused rather than attempted, each naming the next move: switching with unsaved changes, merging anything that is not a fast-forward, committing with no configured author, and deleting the branch you are standing on. `git2` is built `--no-default-features`, so the libgit2 underneath has no HTTPS or SSH transport compiled in at all — remote git is structurally absent rather than merely unimplemented.

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
- `list_scenes()` / `read_scene(id)` — the book in reading order, and one scene's prose with every record it names
- `iceberg()` — what of the world reaches the page, sorted underbuilt-first

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

Six operations cover the realistic surface: `create_entity`, `create_event`, `add_fact`, `remove_fact`, `set_existence`, `set_event_date`. **Omission is never destructive** — an end of `set_existence` you leave out stays as it was, and `"?"` is how you clear one. The two layers disagreed about this until slice 4.5: the applier left omitted ends alone while the tool schema cleared them, so an agent correcting a death date would silently erase the birth date it never mentioned.

**The queue is worth more than a list of pending edits because of impact analysis.** Before accepting anything, `wb-propose` simulates the change in memory, re-runs the consistency engine, and diffs the findings — so the writer sees which contradictions a proposal settles and which it creates. The example world ships one of each:

| Proposal | Effect |
|---|---|
| *Aldric died of his wounds within the year* | settles 2 · breaks nothing |
| *Vashen ruled from Marrow before the conquest* | settles 0 · **adds 2 definite** |

Two, not one, since slice 5: the same question — was Aldric alive at the siege? — is asked by the event record listing him among the participants and by chapter twelve naming him on the page. Giving him another year settles both, which is the impact analysis reaching the prose.

The second reads perfectly plausibly and is wrong twice over — Marrow does not exist until `0602~`, and Vashen already has a capital across those years. It is caught before it lands, not after.

Findings are matched on what they are *about* (rule + subject + related), never on their wording — a proposal that shifts a date would otherwise look like it had resolved a problem and introduced an identical one.

**Writing is deliberately conservative.** Rendering happens in full before anything is written, so a proposal that cannot be applied completely is not applied in part. Prose bodies are preserved verbatim. Decided proposals stay on disk carrying their status — provenance that matters more once agents are filling the queue.

~~**Two stated limitations.** Frontmatter is rewritten canonically, so comments inside it do not survive and a one-line change can produce a wide diff.~~ **Fixed in slice 4.5.** Applying now patches a record in place: comments, inline style, key order, and keys the model does not model all survive, and a one-field change is a one-line diff. A file using YAML the writer will not risk patching — anchors, aliases, merge keys — is reported as such before anything is written and falls back to the old canonical rewrite. `Error::WouldDropKey` still guards that fallback, because a key the model does not understand must be *refused* rather than dropped: this tool holds people's life's work.

### 7.3 Shippable skills

Skills are where worldbuilding *methodology* lives, distributable and extensible without shipping app updates. All six ship in [`skills/`](skills/):

- `consistency-audit` — reason over deterministic violations, separate bugs from mysteries
- `world-from-notes` — ingest an existing 40-page doc into structured entities (**the single biggest adoption barrier in this space is re-entering years of notes by hand**)
- `chapter-canon-check` — read a linked scene, check it against world state at that date
- `culture-from-phonology` — names consistent with an existing language
- `succession-crisis` — plausible dynastic fallout
- `iceberg-check` — what have you overbuilt that never surfaces in the manuscript?

Two of them shipped in slice 3 carrying written apologies for what the tools could not yet answer. `iceberg-check` said *"until scenes are linked to the manuscript, this measures internal connectedness … not what surfaces on the page"*, and `chapter-canon-check` had to tell an agent to go find the prose on the filesystem itself. Slice 5 deleted both paragraphs and replaced them with the judgement calls that are actually left — which `standing` you are looking at, and the two opposite causes of a low ratio.

The tools say what an agent *can* do; the skills say what is worth doing, and — more often — what to refuse on the writer's behalf. Every one ends the same way: **offer, do not decide.** A name is taste, a mystery is a plot, and where to spend the next hour is the writer's call.

The methodology worth naming here is `world-from-notes`'s: **preserve the vagueness.** Notes are vague because worlds are vague, and the instinct to tidy that away is the failure mode. "Roughly 600 AR" becomes `0600~`, never `0600`; "after the Sundering" becomes `>@evt_sundering`, never an invented year. Every piece of precision an agent adds is a fact the writer never wrote and will later have to discover is wrong.

`crates/wb-mcp/tests/skills.rs` asserts that every tool the server exposes is named by at least one shipped skill, so a tool never ships without methodology attached. It has already caught one: `list_proposals` had no skill telling an agent to check the queue before adding to it.

### 7.4 Staying honest against the disk

The server is long-lived and the writer is editing these same files in their own editor while it runs — that is the point of files-as-source-of-truth. A server that loaded once at startup would confidently answer with yesterday's canon.

So every call fingerprints the world tree (path, size, mtime) and reloads when anything moved. The fingerprint is a walk; the reload is a walk plus a parse, and skipping the parse is most of the win. A reload that fails — a half-saved file mid-keystroke — is not fatal: the last good world keeps answering and the error says so rather than a stale answer passing as fresh.

---

## 8. Manuscript integration

The app **never owns or edits prose.** Writers are attached to Scrivener, Obsidian, and Word, and will not move. The link is one-way by construction: there is no function anywhere in `wb-story` that writes, which is the cheapest way to keep a promise like that.

A scene is a first-class record — a scene *is* an interval with a POV character and a location:

```yaml
# scenes/the-breach.yaml
id: scn_the_breach
name: The breach
date: "@evt_siege_of_marrow"       # anchored, so re-dating the siege drags the chapter
location: place_marrow
on_page: [pol_vashen, ter_vale_of_corrath]
prose: ch12-the-siege.md#the-breach  # read-only link, relative to the manuscript root
```

And the root is declared once, in `world.yaml`:

```yaml
manuscript:
  root: ../manuscript      # the only path in a world allowed to leave its folder
```

> **Revised during implementation, twice.** The sketch made a scene `primitive: event` carrying `pov` and `source`. Both halves turned out to be traps. `Event.source` is `#[serde(skip)]` — the file the record was loaded from — so a `source:` key would have parsed as unknown, been dropped, and left the writer looking at a link nothing read; the key is **`prose:`**, and a scene that says `source:` is refused by name. And folding scenes into `world.events` would have put the book's chapters onto the history track and changed what `event_count` means, so `Scene` is its own record type in `world.scenes`, filed under `scenes/` exactly as §6 reserved.
>
> Declaring the root in `world.yaml` rather than writing `../manuscript/ch12.md` on every scene was the other change: one escape hatch, visible in a diff, and when the book moves one line moves with it.

This gives you, with zero editor built:
- The story rendered on the timeline against world history, in its own band
- The book's route lit up on the map, in **reading** order — which is derived from the manuscript itself, so a flashback shows as the path doubling back rather than as an error
- Prose checkable against canon (§5's `scene-contradiction`)
- The agent seeing both world and manuscript (`list_scenes`, `read_scene`, `iceberg`)

**Derived feature worth calling out:** scanning linked prose for entity mentions yields a **"surfaced" flag** — which parts of the world actually appear on the page. That's a live [iceberg ratio](https://andreacerasoni.com/blog/iceberg-method): the 10% above water vs the 90% below. No other tool can show a writer that.

**The matcher is deliberately conservative, and that is the whole design.** It counts a record's `name`, its declared `aka` spellings, and `[[wikilinks]]`, matched on whole words — never a single word out of a multi-word name, because "The Vale of Corrath" would then be found in every sentence containing "vale". A wrong ratio is worse than no ratio: a writer will act on it. So every hit carries the sentence it came from, and the caveat travels on the payload rather than living only in a skill — a low ratio means either the world is not on the page *or* the world has not been told what the page calls things, and those need opposite responses.

`aka` is a first-class list rather than a fact. As a fact it would be multi-valued, so every world would have to remember to list it under `rules.multi_valued` or watch the engine call two nicknames a contradiction.

The scan walks the prose once, testing 1..=K-word windows at each position, so it is O(words × K) and independent of how many records exist. `World::search` runs the other way — one query against every record, matching on `contains` with no word boundaries — which is right for a search box and wrong for this.

---

**Slice 6 addendum: the exported bible contains no scenes.** A scene points into a
manuscript the recipient either already has or should not have, and a document about the
world is not the place for the structure of the book that reveals it. What *is* there is
[`Scope::OnThePage`], which turns the same mention scan around: hand a reader only the
records the book has actually named them, which is spoiler-free by exposure rather than by
date. The other two scopes are everything, and everything as it stood on one day — the
second being a gazetteer that reads as though a chronicler wrote it that year, because
every fact in it is `world.at(day)`.

[`Scope::OnThePage`]: crates/wb-export/src/lib.rs

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

**Respect the iceberg.** ~90% of a world never reaches the page. The tool must comfortably hold far more than it shows and never imply that empty fields are incomplete work. Slice 5 makes this measurable without making it a scold: the story panel reports what surfaces, sorts *underbuilt* first because that is the only quadrant naming work worth doing, and says of the rest that stubs are not debt.

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
| Git | `git2` (libgit2), `--no-default-features` | Branching histories. Vendored libgit2 + libz, and **no network transport compiled in** |
| Markdown (export) | `pulldown-cmark` | Record bodies in the exported bible |

---

## 11. Roadmap

| Slice | Contents |
|---|---|
| **1 — Temporal spine** ✅ | Interval model, file storage, timeline scrubber, map with hand-drawn vector regions, fuzzy dates + relative anchoring, calendar engine |
| **2 — Integrity** ✅ | Deterministic consistency engine ✅ · extensible ontology ✅ · proposal/draft layer ✅ |
| **3 — Agent surface** ✅ | MCP server ✅ · tool surface ✅ · shipped skills ✅ · notes ingestion ✅ |
| **4 — Map depth** ✅ | Coastline vectorization ✅ · cell substrate ✅ · heightmap ✅ · climate ✅ · rivers ✅ · biomes ✅ |
| **4.5 — Authoring** ✅ | Format-preserving writer ✅ · record editor ✅ · click-to-place markers ✅ · polygon drawing ✅ · events ✅ · delete with reference check ✅ |
| **5 — Story** ✅ | Scene records ✅ · external prose linking ✅ · mention scanning ✅ · surfaced/iceberg view ✅ · scene band and story window ✅ · story path on the map ✅ |
| **6 — Depth** ✅ | Save points and what-ifs ✅ · world-level comparison ✅ · lineage and dynasty view ✅ · export & publish ✅ · open your own world ✅ |

Slice 1 is deliberately the shortest path to the moment that proves the thesis — and it forces the hardest decisions (interval semantics, fuzzy date resolution, scrub performance) while the codebase is still small enough to throw away.

### Status

| Crate / step | State |
|---|---|
| `wb-core` — calendars, fuzzy dates, anchor resolution, Allen intervals | **done**, 56 tests |
| `wb-store` — file format, loader, world assembly, time-indexed queries, search, format-preserving writer | **done**, 89 tests |
| `wb-check` — the consistency rules, and the vocabulary the seventh borrows | **done**, 17 tests |
| `wb-propose` — review queue, impact analysis, applier | **done**, 18 tests |
| `wb-mcp` — MCP server, 21 tools, notes ingestion, terrain queries | **done**, 46 tests |
| `wb-terrain` — the eight-stage map pipeline | **done**, 123 tests |
| `wb-story` — manuscript reader, mention scanner, iceberg, canon check | **done**, 29 tests |
| `wb-git` — standing, history, save points, what-ifs, reading a revision back out | **done**, 20 tests |
| `wb-export` — a world as one self-contained HTML document | **done**, 14 tests |
| `skills/` — six shipped methodologies | **done** |
| `examples/vashen` — a working seed world | **done**, 12 entities, 3 events, 3 scenes, 2 proposals, 1 notes file, 1 map, 2 chapters |
| Tauri commands — query surface, the direct write path, the story, versions and publishing | **done**, 27 tests |
| Svelte map with five terrain layers, lineage chart, timeline, inspector, findings, review queue, record editor, story panel, version panel, export panel | **done** |
| SQLite index | **not needed** — see below |

**457 tests** across the workspace. Clippy clean under `-D warnings`; `svelte-check` reports 0 errors and 0 warnings.

**All six slices are complete.** What remains open is named in §12 and is open on purpose: polygon morphing, shared-border authoring, and the half of the timeline-scale question a two-position toggle does not answer.

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

### Slice 4.5 verified in the running app

Writing to a world is the first thing this app does that can destroy work, so the check is
what reached the *files*, not what appeared on screen.

| What was done | What it proves |
|---|---|
| Opened Marrow's record through the map, in the editor | The authored dates arrive as authored: `0602~` still approximate, `0800` still a plain year, `@evt_siege_of_marrow` still an anchor, `9000` still typed `int` |
| Changed one population and saved | `git diff` on the real file is **one line** |
| Clicked the same pixel at 100%, zoomed, panned, and at 900% | The marker renders back under that pixel every time, to within 0.8 px — the *same* 0.8 px at every zoom, so nothing drifts |
| Pressed, dragged 48 px, released — in marker mode | No marker placed; the map panned. The regression test for `aee5e97` |
| Dragged a polygon vertex | The vertex moved and the entity group's `transform` was byte-identical before and after |
| Created a record from a blank form | `place_greyford` derived from the name, and the file it wrote is indistinguishable from a hand-written one |

Every gesture went through a full `pointerdown → pointermove → pointerup → click` sequence
dispatched at real coordinates, on the element `elementFromPoint` actually returns. A bare
`.click()` skips hit-testing and pointer capture — which is exactly how the bug in
`aee5e97` survived a whole slice of checking.

Four things the work turned up, none of which a test would have caught:

- **The validation effect fed itself.** It read `phase` and also wrote it, so
  reviewed → dirty → validating → reviewed forever, and the panel never left "checking…".
  Reading the draft as the only tracked dependency and doing the rest under `untrack` fixed
  it. Svelte's fine-grained reactivity makes this easy to write and invisible until you
  drive the app.
- **Finishing a vertex drag left the whole mode.** `onmodedone` was doing double duty as
  "this gesture committed" and "leave the mode", so the handles vanished after every
  adjustment. They are different events.
- **A `<select>` breaks tauri-pilot's screenshot.** Bisected to the element. Replaced with a
  text input and a `<datalist>`, which is a better fit anyway: the type vocabulary is open,
  so a dropdown made the UI stricter than the data model and needed an escape hatch bolted
  on to undo that. The app now has no dropdowns, as it had none before.
- **A click carried sixteen significant digits, and new files came out in machine style.**
  `marker: [0.2190476190476191]` as a block sequence, in a folder where every other record
  says `marker: [0.43, 0.40]`. Coordinates round to four decimals — finer than the raster
  has pixels — and new records are written through the same emitter the patcher uses.

### Slice 5 verified in the running app

The risk here is different again: nothing this slice writes can destroy work, but the
number it reports can be *confidently wrong*, and a writer will act on it.

| What was done | What it proves |
|---|---|
| Compared the story panel against `--example iceberg` | 73%, 8 of 11, 3 scenes read, same records in the same order. Two implementations of one number agreeing is the reason for computing it twice |
| Read every mention's excerpt | Each of the eight surfaced records shows the sentence it was found in. Two records — Maren and Vashen — surfaced only because of a declared `aka`, which is the mechanism earning its place |
| Toggled `just the story` | The axis goes from 169,689 days to 6,119, and three scenes spread from 89.5–92.5% of the track to 8.3 / 57.4 / 90.7%. §12.4's problem, in numbers |
| Opened a scene from the map | The prose resolves live: `→ ch01-the-wall.md · "The gate at dusk" · 239 words`, with the opening lines beneath it |
| Saved one scene's date | One changed line. The three-line comment above it survived, and `examples/vashen` was byte-identical after `git checkout` |
| Created a scene with Aldric as POV a year after the siege | Reports **opens**, not breaks — his death is `0811~`, so the world permits it and does not confirm it |
| Pointed `manuscript.root` at a folder that is not there | The chip reads *manuscript missing* rather than a false 0%, the panel names the folder, the prose finding drops away, the scenes still render as records, and nothing throws |
| Two scenes at the same location | The stop reads `1·3` — grouped, because the first version drew two circles on the same pixel and a book that visited Marrow twice looked like it visited once |

Four defects came out of driving it and none out of reading it: a header that clipped its
own counts once six chips shared a row, a toggle that labelled its state so a lone button
offered nothing, co-located scenes hiding each other, and `+ scene` deriving no id because
scenes were missing from the prefix table.

The rule from slice 4.5 held throughout: **never `tauri-pilot click` inside the map.** Every
gesture went through `elementFromPoint` and a full `pointerdown → pointerup → click` on
whatever it actually returned.

### Slice 6 verified in the running app

The risk moved again. This is the first slice where a button can destroy work that was
never in the app, and the first where a comparison a writer *acts on* — keep this
experiment, or throw the week away — is computed from two worlds at once.

**Read-only, on the world that ships with the tool.** `examples/vashen` is a folder inside
this repository, so the version panel says so in a sentence naming `worldbuilder`, offers
no branch button, and still gives the whole read-only half:

| What was done | What it proves |
|---|---|
| Opened the version panel on the example world | *"This world is a folder inside worldbuilder. Branching would move that whole repository…"* — and history below it, filtered to the four commits that touched the world subtree out of ten that touched the repo |
| Compared against the slice-4 commit | **3 scenes added, 4 records changed** — each naming its fields (`aka +2`, `aka +1`) rather than its lines — and **1 finding opened**, the chapter-twelve contradiction that could not exist before scenes did |
| Compared against the newest subtree commit | *"Nothing differs. Not one record, not one date."* This is the load-bearing one: had the materialized side lost its manuscript, the two prose findings would have shown as newly opened, and the panel would have been confidently wrong |
| Checked the folder afterwards | `.worldbuilder/compare/` empty, `git status --porcelain examples/` unchanged. One comparison must not make the next branch switch impossible |

**The whole what-if loop, on a real world folder that is its own repository.** A copy of
the example world with `manuscript/` beside it, `git init`, opened through the new
`open…` field:

| Step | Result |
|---|---|
| Started `what-if/aldric-lived` and switched to it | Header moves to *on what-if/aldric-lived* |
| Changed Aldric's death to `@evt_siege_of_marrow+1y` **in an editor outside the app** | Panel reads *1 unsaved*, naming the file — with `.worldbuilder/` excluded, though git sees it as untracked |
| Made a save point | *saved as 3b42244*; the branch reads *1 ahead* |
| Compared against canon | **Aldric Vane · existence · death 810 days later**, and *both* open questions settled — one from the record, one from chapter twelve |
| Fast-forwarded canon, then switched to it | The consistency chip goes from **2 open questions** to **consistent** |
| Deleted the what-if | Two presses; the second is the one that acts, and it states how many save points would become unreachable |
| Threw away an unsaved change | *threw away 1 change*, and the file is back to what the save point said |
| Tried to switch branches with an unsaved change | Refused, naming the file: *"…has changes you have not saved. Make a save point first, or throw them away."* |

**Publishing.** Exporting *as it stood* on `0810-01-01` produced a 547 KB file containing
Marrow's population as **9,000** and not 3,100, and no Siege of Marrow at all — the siege
has not happened yet at that instant, and the document is written in the voice of the year.
Pressing write twice refused the second time and re-labelled itself *replace it*.

Three defects came out of driving it, none out of reading it. A duplicate-key crash that
took the whole lineage chart down, because Marrow is a duchy's capital *and* a duke's seat
from the same oath — one record legitimately holding two things over the same days. Row
labels printing `from 0599-12-21` for a record that says `0602~`, which is the fuzz
envelope's near edge and not remotely what the record means. And the same false precision
in the exported bible, one layer down — the second occurrence is why [`wb_store::phrasing`]
exists rather than three surfaces each formatting dates their own way.

[`wb_store::phrasing`]: crates/wb-store/src/phrasing.rs

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
4. **Timeline scale range** — 4,000 years of history and a six-week story on one axis. **Half-closed in slice 5**: scenes have their own band, and a `just the story` toggle clamps the axis to the manuscript's own date range — on the example world that turns three scenes smeared across 3% of the track into three spread across 82% of it. That is a two-position toggle, not the era → century → year → day zoom this question asks for, and the event-density minimap remains unbuilt. The half that is done is the half that made the scene band readable. **Slice 6 did not close the other half either** — the lineage chart sidesteps it by fitting its own axis to the records on screen and coupling to the timeline by the scrubbed *day* rather than by pixels. Three actors over four thousand years is two percent of the main track; a second chart that reproduced that would have been a second unreadable one.
5. **Constraint solver complexity** — fuzzy anchors form a DAG needing cycle detection and interval propagation. Keep it simple; resist building a general temporal reasoner.
6. **Blank page** — needs seed worlds and templates, or slice 1 demos to an empty screen.
7. **Scope discipline** — every section above is a product on its own. ~~The map pipeline in particular is a rabbit hole that can swallow months.~~ **Survived** (§11): pure stages, a build-product output, and an ASCII plot to tune against. The rabbit hole turned out to be the parameters, not the code.
8. **Version control is somebody's real repository.** Slice 6 is the first place a button can lose work that was never in this app. Three controls hold it: mutations are gated on the world folder being the repository root, every refusal has a test written before its success path, and the two destructive actions state the count of what disappears before the second press. Merges that are not fast-forwards are refused outright — a half-merged world folder is the worst state this app could hand back, and conflict resolution is a real git client's job.

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
