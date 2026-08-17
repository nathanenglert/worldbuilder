---
name: iceberg-check
description: Find what a writer has overbuilt and never used, and what the story leans on that was never built. Use when someone feels stuck, is worldbuilding instead of writing, or wants to know what actually matters in their world.
---

# Iceberg check

Roughly 90% of a world never reaches the page, and that is correct — the submerged part
is what makes the visible part feel solid. The failure mode is not having too much. It
is building depth in the wrong place: elaborate detail where the story never goes, and
nothing at all where it lives.

## Two questions, and the second is the important one

**What is overbuilt?** Records with many facts and much prose that nothing else in the
world points at.

**What is underbuilt?** Records the world leans on constantly that have almost nothing
in them. This is where a writer actually gets stuck, and it is invisible without asking.

## Measure the page, not just the world

`iceberg` is the whole report in one call, and it is the one to start with. For each
record it gives you what no file states:

- `mentions` — times the linked prose actually names it, with `first_seen`, a sentence
  you can read to check the count
- `scenes` — which scenes it appears in
- `referenced_by` / `appears_in` / `cast_in` — what the *records* say: facts pointing
  here, events naming it, scenes listing it in their cast

`cast_in` and `mentions` are different measurements and the gap between them is
interesting on its own: a scene listing somebody who never appears in its prose is
usually a note the chapter outgrew.

Records come back `underbuilt` first, already sorted. Together with fact and prose
volume, that is the four quadrants:

| | Few references | Many references |
|---|---|---|
| **Much detail** | Overbuilt — beautiful and unused | Load-bearing — the real spine |
| **Little detail** | Fine. Most of a world is stubs, and stubs are not debt | **Underbuilt** — the story keeps reaching for something that is not there |

Report the bottom-right quadrant first. A place named in six events with no prose and no
facts is the single most useful thing you can hand a writer.

## Bring in the prose

`list_scenes` gives you the book in reading order. `read_scene` gives you one scene's
prose and every record it names. Use them when the writer asks about a particular
chapter, or when a number in `iceberg` looks wrong and you want to see the page it came
from.

If the world also has `notes/`, `list_notes` and read them: something mentioned
constantly in the notes but thin in the world is underbuilt in the same way.

**Know which measurement you have.** `iceberg` reports `standing`, and it changes what
you can honestly say:

- `linked` — the ratio is real. This is what surfaces on the page.
- `unlinked` — no manuscript is attached, so every record reads as submerged. That is
  not a finding about the world. Say so, and offer `manuscript.root` in `world.yaml` as
  the thing that would make the question answerable.
- `root_missing` — the book moved. Tell them, do not report 0%.

**A low ratio has two causes and they need opposite responses.** Either the world is
genuinely not on the page, or the world has not been told what the page calls things —
a character the prose only ever names by a first name reads as absent until that
spelling is in their `aka`. Check `first_seen` on the records that *did* surface. If the
ones that surfaced are all full-name matches, suspect the second cause and offer to add
aliases before concluding anything about the writer's book.

## Never call anything waste

Empty fields are not incomplete work, and an unused culture is not a mistake — it is the
part of the iceberg doing its job. Frame the whole report as *where the next hour is
best spent*, not as an audit of what was wasted.

The output a writer can act on is short:

1. Three records the story leans on that have nothing in them, with what each is missing.
2. Anything genuinely orphaned — referenced by nothing, appearing in nothing — offered as
   a question, not a deletion. It may be the seed of the next book.
3. What is load-bearing, named, so they know what not to casually change.

Do not file proposals from this. It is a report about where to work, and where to work
is not a decision an agent gets to make.
