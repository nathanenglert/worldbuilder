---
name: chapter-canon-check
description: Check a chapter or scene of prose against the world as it stood on that date. Use when a writer asks whether a passage contradicts their canon, or wants a draft checked before it goes out.
---

# Chapter canon check

The manuscript is not in the tool and never will be. Writers are attached to Scrivener,
Obsidian, and Word, and will not move. So this reads prose from wherever it lives and
checks it against the world at the date it happens.

## Establish the date first

Nothing else works until you know when the scene is set. In order of preference:

1. The scene states it.
2. It sits between two events you can name — then it is `>@evt_a` and `<@evt_b`, and
   you check against both ends.
3. Ask. Do not guess a year in order to have something to check against; every finding
   downstream would inherit the guess.

`resolve_date` turns whatever you settle on into a day.

## Then take the world at that moment

```
world_at(date)          → who and what existed, and which facts were live
territory_at(date)      → who held what
timeline(from, to)      → what had just happened, and what had not yet
```

Read the whole snapshot before reading the prose closely. Most contradictions are of the
form "this character is doing something a dead person cannot do" or "this city is under
a flag it did not fly yet", and both are visible in the snapshot alone.

## What to check, in rough order of how often it is wrong

- **Who is alive.** `existence: "no"` means the record is absent from the snapshot
  entirely. A named character who is not there is either dead, unborn, or misspelled.
- **Who holds what.** Compare every place named in the prose against `territory_at`.
  Border changes are the single most common canon slip.
- **Titles and offices.** A fact's window is when it held. "Duke of Corrath" in 0805 is
  wrong if the title starts in 0808.
- **What has happened yet.** Characters referring to events that have not occurred.
  `timeline` with a `to` of the scene date is the check.
- **Distances and travel time** against the calendar, if the world records geography
  well enough to support it. Often it does not — say so rather than inventing a rate.

## Report `maybe` as `maybe`

When a fact comes back `certainty: "maybe"`, the world genuinely does not settle the
question at that date. The prose is then **not** contradicting canon — it is choosing
one reading of it. That is a writer's prerogative and often exactly what they are doing
on purpose.

Say so plainly: *"The Vale is contested through Verdant 812 — the scene has Corrath
still holding it, which the world permits but does not confirm. If you want that fixed,
the siege needs a day rather than a month."*

That sentence is worth more than a list of violations, because it tells them what to
change in the **world** to make the prose safe.

## Offer, do not file

A canon check produces findings, not proposals. If the writer decides the prose is right
and the world is wrong, *then* propose the world change — and run `check_changes` first,
because re-dating one event moves everything anchored to it.
