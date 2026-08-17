---
name: succession-crisis
description: Work out who claims a title when a ruler dies, and what plausibly follows. Use when a writer kills someone off, asks who inherits, or has a succession-gap finding to resolve.
---

# Succession crisis

A gap in a single-valued attribute — a throne with nobody on it — is the most
structurally interesting thing that can happen in a world, and the engine finds them for
free.

## Find the gap

```
succession()                     → everything in this world that changed hands
succession(key: "title|title|Duke of Corrath")   → just that one
```

Each entry is the holders in the order they held it, plus two lists that are the whole
point:

- **`gaps`** — nobody held it. The same thing `check_consistency(rule: "succession-gap")`
  reports, seen from the other side: the rule asks whether *a person* ever held no title,
  and this asks who held *the title*, which is the question a crisis is actually about.
- **`unsettled`** — more than one holder is possible. **This is not a rival claim.** It
  is two vague dates meeting, and the world genuinely does not say. Resolving it by
  picking a side is inventing canon; saying so is the finding.

`succession()` covers anything passed between records, not only titles — a territory
changing hands between empires comes out the same shape as a duchy passing down a line,
because in this model it *is* the same shape.

## Assemble the claimants

```
lineage(id, depth: 3)            → ancestors and descendants, with lifespans
get_entity(id)                   → `children`, `referenced_by`, `appears_in`
query_entities(alive_at: date)   → who was actually alive at the moment of the gap
```

`alive_at` matters more than it looks. A claim by someone who died before the vacancy is
not a claim, and a claimant born after it inherits the aftermath, not the throne.

For each candidate, establish and write down:

- **Descent** — through whom, and how many steps.
- **Alive on the day?** Not "alive around then". Use the date.
- **What else they hold**, from their facts. A claimant who already rules elsewhere is a
  union or a war depending on the world.
- **Where they were.** `timeline(involving: id)` around the date. Presence at the event
  that caused the vacancy is the difference between an heir and a suspect.

## Apply the world's own law, if it has one

Look for a succession rule recorded as a fact or in a polity's prose body before
assuming primogeniture. Elective, agnatic, ultimogeniture, and partible inheritance all
produce entirely different crises, and defaulting to eldest-son is how generated content
flattens a world into generic medieval Europe.

If the world records no rule, **that is the finding**. Say so, present what each rule
would produce, and let the writer choose — you have just handed them a decision that
shapes centuries of their history, which is worth more than an answer.

## Then the fallout

Only once the claimants are real. Keep it grounded in what the world says:

- Who has the **military** facts, the walls, the coin.
- Which **neighbours** gain from a weak claim, per `territory_at` at the date.
- What the **interregnum** does to anything else anchored to the office — treaties,
  ownership, offices held "under" the ruler.
- Who benefits from the ambiguity lasting. A contested succession that nobody resolves
  is often better story than one that snaps shut.

## Proposing it

A resolved succession is usually two changes: close the old holder's title window, open
the new one at the same date. `[from, to)` intervals are half-open, so the two meet
exactly and the gap closes cleanly — and `succession()` will show no hole and no
unsettled stretch afterwards, which is the check that it actually closed.

Anchor both to the event that caused it — `@evt_siege_of_marrow` — rather than a year,
so re-dating the event drags the succession with it.

Run `check_changes` first. Closing one gap frequently opens another somewhere the writer
was not looking.
