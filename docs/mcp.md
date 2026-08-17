# Connecting an agent

Worldbuilder exposes a world through a local MCP server. You bring your own agent —
Claude Code, Claude Desktop, Cursor, anything that speaks MCP. There are no API keys in
the app and no inference cost on anyone.

This is what makes *"AI isn't the main driver"* structurally true rather than a promise:
the app is fully functional with nothing attached. The server is an optional client of
the data model, never a layer inside it.

## Two things to know before you start

**It cannot write to your world.** Every write tool files a proposal into `proposals/`,
and you accept or reject it in the app. There is no accept tool and no way to add one
from the agent side. You lose trust in a tool like this in exactly one bad session, and
the queue costs nothing — canon-versus-speculative staging is something writers want
anyway.

**Omission is never destructive.** An end of `set_existence` you leave out stays as it
was; send `"?"` to clear one. Correcting a death date must not quietly erase a birth date
nobody asked about.

**Accepting keeps your formatting.** A proposal is applied by patching the record in
place, so comments, inline style, and keys the model does not understand all survive, and
the diff you review is the diff you get. If a file uses YAML the writer will not risk
touching — anchors, aliases, merge keys — the queue says so before you accept.

**It never resolves your uncertainty.** A fact that is only *maybe* true at a date comes
back as `maybe`. A `possible` consistency finding comes back as possible, because that
is the shape a deliberate mystery takes. The engine detects; judgement is the agent's
job, and yours.

## Install

```sh
cargo build --release -p wb-mcp
```

The binary lands at `target/release/wb-mcp`. Copy it somewhere on your `PATH`, or use
the full path below.

## Register it

**Claude Code:**

```sh
claude mcp add worldbuilder -- /path/to/wb-mcp /path/to/my-world
```

**Claude Desktop** — in `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "worldbuilder": {
      "command": "/path/to/wb-mcp",
      "args": ["/path/to/my-world"]
    }
  }
}
```

The world folder is the one containing `world.yaml`. It can also come from the
environment as `WORLDBUILDER_WORLD`, which some clients find easier:

```json
{
  "mcpServers": {
    "worldbuilder": {
      "command": "/path/to/wb-mcp",
      "env": { "WORLDBUILDER_WORLD": "/path/to/my-world" }
    }
  }
}
```

Try it against the bundled world first — `examples/vashen` has eleven records, three
events, a contested border, and one open question, which is enough to see what every
tool does.

## The tools

**Reading** — unrestricted.

| Tool | |
|---|---|
| `describe_world` | Calendar, date syntax, type and attribute vocabulary, where consistency stands. **Always the first call.** |
| `world_at` | Everything true at one instant — the map's own query |
| `get_entity` | One whole record, optionally as it stood at a date |
| `query_entities` | Filter by type, name, geometry, or existence at a date |
| `timeline` | Events in a window |
| `territory_at` | Map geometry with live ownership claims |
| `describe_place` | What the ground is like at a point, or under a record with a marker |
| `find_sites` | Candidate locations matching the ground — on a river, coastal, a biome, near somewhere |
| `lineage` | Ancestors and descendants with lifespans |
| `check_consistency` | Every deterministic contradiction, with its certainty — including the ones found in linked prose |
| `search` | Ranked full-text over ids, names, types, fact values, and prose |
| `resolve_date` | Check a date expression before writing it into a proposal |
| `list_notes` / `read_note` | Source documents in the world's `notes/` folder |
| `list_scenes` | The book in reading order — dates, point of view, location, and the prose each scene links to |
| `read_scene` | One scene's prose, plus every record the passage names |
| `iceberg` | What of the world reaches the page, sorted underbuilt-first |

**Writing** — routed to the review queue, never to canon.

| Tool | |
|---|---|
| `check_changes` | Dry run: what a change would settle, what it would break. Writes nothing |
| `propose_changes` | File a proposal for you to decide on in the app |
| `list_proposals` | The queue, with each proposal's measured impact |

## The notes folder

Put your existing prose notes in `notes/` inside the world folder and the agent can read
them. That is what makes ingestion work with only this server attached — no filesystem
access needed.

Reads are scoped to that folder, checked *after* canonicalization, so a symlink pointing
out of it is refused as firmly as `../../.ssh/id_rsa` is. This server reads your notes,
not your disk.

## The ground

If your world declares a `map:` in `world.yaml`, `describe_world` comes back with a
terrain summary too, and two more tools open up.

The point of them is placement. Notes place things relative to other things — *upriver
from Marrow*, *on the coast north of the Vale* — and a `marker` is two numbers.
`find_sites(on_river: true, near: "place_marrow", within: 0.12)` turns the first into
candidates for the second, each reported with its biome, its rainfall, and how far it is
from the anchor. `describe_place` checks one.

Two things the payloads say out loud, because both are easy to get wrong:

- **Terrain is not canon.** It is derived from your map image and the settings under
  `map.terrain`, cached in `.worldbuilder/`, and rebuilt whenever either moves. There is
  no way to propose a change to it, by design — if the map is wrong, the map is what
  changes.
- **Coordinates are normalized `0..1` over the image, and `y` increases southward.**
  North is *smaller* `y`. Getting that backwards mirrors every placement.

## Skills

Tools describe what an agent *can* do. The [shipped skills](../skills/) describe what is
worth doing — and what to refuse on your behalf. Six of them cover auditing, ingestion,
canon-checking, naming, succession, and finding what you have overbuilt.

## When something is wrong

**"eval timed out" / no tools listed.** The client cannot see the server. Check the path
in your config is the binary and the world folder, in that order.

**"cannot open world at …".** The folder needs a `world.yaml` at its top level. The path
is printed in the error; the server writes all diagnostics to stderr, which your client
shows in its own logs.

**Answers look stale.** They should not be. The server fingerprints the world tree on
every call and reloads when anything moved, so editing a record in your own editor
mid-session is picked up on the next question. `describe_world` reports a `reloads`
count if you want to confirm it.

**Nothing ever appears in the app.** Filing a proposal is not accepting one. Open the
review queue.
