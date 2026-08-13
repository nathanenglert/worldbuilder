---
name: world-from-notes
description: Turn a writer's existing prose notes into structured Worldbuilder records. Use when someone has years of worldbuilding in documents and wants it in the tool without retyping it.
---

# World from notes

Re-entering years of notes by hand is the single biggest reason people abandon tools
like this. This is the job that earns the tool its place.

## Read before you write

```
describe_world     → calendar, date syntax, the type and attribute vocabulary in use
list_notes         → what source material there is
read_note          → one at a time
```

`describe_world` is not optional. Its `attributes` list tells you what this world
already calls things. A world that records rulers as `owner` does not want `ruled_by`
alongside it — the second name is invisible to every query the first one answers, and
the consistency engine cannot see a conflict between two attributes it thinks are
unrelated.

Same for `types`. Use a declared one. If nothing fits, say so and propose the new type
to the writer in words before using it.

## Preserve the uncertainty — this is the whole discipline

Notes are vague because worlds are vague. Your instinct will be to tidy that away.
Don't. Every piece of precision you invent is a fact the writer never wrote and will
later have to discover is wrong.

| The note says | Write | Never |
|---|---|---|
| "roughly 600 AR" | `0600~` | `0600` |
| "sometime in the 800s" | `0800..0899` | `0850` |
| "after the Sundering" | `>@evt_sundering` | a made-up year |
| "a generation before Aldric" | `@act_aldric_vane.birth-1g` | arithmetic you did yourself |
| nothing about when | `?` or omit | anything at all |

`?` is a real answer. An entity with no dates is perfectly valid and can be filled in
later — that is what lets someone build bottom-up from one tavern.

When a note contradicts canon, **do not resolve it.** File it as a question for the
writer, or propose the change with the contradiction named in the note. The notes may
be newer than the world, or the world may be newer than the notes; you cannot tell.

## Structure the pass

Work one note, or one coherent section, at a time. For each:

1. Extract the **entities** first — people, polities, places, things. Ids follow the
   world's convention (`act_`, `pol_`, `place_`, `thing_`).
2. Then **events**, which are dated occurrences. Events carry no effects: a conquest
   does not change ownership by itself. The owner fact anchors to it by date.
3. Then **facts**, each with the window it holds over. `from` and `to` anchored to
   events where the note phrases it that way — `to: "@evt_siege_of_marrow"` survives
   the writer re-dating the siege; a hardcoded year does not.
4. `check_changes` on the batch.
5. `propose_changes` — **one proposal per coherent chunk**, not one per record.

A chapter of notes is one decision a writer can say yes to. Forty proposals is forty
decisions and they will reject the lot rather than work through it.

## Titles and notes on the proposal

The `title` is the claim, not the mechanism: *"House Ferrow holds Greyford, and has
since before Corrath"*, never *"create 3 entities and 2 facts"*.

The `note` cites the source. `From notes/houses-and-holdings.md — the third house,
never recorded.` A writer reviewing forty changes needs to know where they came from.

## Report honestly

Say what you skipped and why. Notes contain things that are not facts about the world —
plot ideas, reminders, arguments the writer is having with themselves. Leaving those out
is correct; leaving them out silently is not.
