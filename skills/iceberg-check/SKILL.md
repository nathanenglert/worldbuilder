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

## Measure connectedness

For each record, `get_entity` gives you the two fields that matter, neither of which any
file states:

- `referenced_by` — other records whose facts point here
- `appears_in` — events naming it as participant or location

Together with fact and prose volume, that sorts a world into four quadrants:

| | Few references | Many references |
|---|---|---|
| **Much detail** | Overbuilt — beautiful and unused | Load-bearing — the real spine |
| **Little detail** | Fine. Most of a world is stubs, and stubs are not debt | **Underbuilt** — the story keeps reaching for something that is not there |

Report the bottom-right quadrant first. A place named in six events with no prose and no
facts is the single most useful thing you can hand a writer.

## Bring in the prose, if it is linked

If the world has `notes/`, `list_notes` and read them: something mentioned constantly in
the notes but thin in the world is underbuilt in the same way.

**Be honest about the limit.** Until scenes are linked to the manuscript, this measures
*internal* connectedness — how much the world refers to itself — not what surfaces on
the page. Those are correlated and not the same thing. A place that appears in one scene
of one chapter can matter more than one referenced by nine records. Say which you
measured.

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
