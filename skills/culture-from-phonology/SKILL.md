---
name: culture-from-phonology
description: Generate names that sound like they belong to an existing culture in a Worldbuilder world, by deriving the sound system from names already in it. Use when a writer needs new people, places, or houses named consistently.
---

# Culture from phonology

A name that sounds wrong is the fastest way to break a reader's belief in a world, and
it is the most common failure of generated content. The fix is not a bigger name list.
It is deriving the actual sound system from what the writer already wrote, and staying
inside it.

## Gather the corpus first

```
query_entities(primitive: "actor")     → people
query_entities(primitive: "place")     → settlements, regions
query_entities(primitive: "polity")    → houses, kingdoms
search(text)                           → anything else that reads as a name
```

Separate them by culture before analysing. A world usually has more than one, and
averaging across them produces names that belong to none. If the world does not record
which culture a name belongs to, group by polity or region and say that is what you did.

Look for a `language` record too — `query_entities(primitive: "thing")`. If one exists,
read its prose. Writers put naming conventions there.

## Derive, do not guess

From the corpus, write down explicitly:

- **Phoneme inventory.** Which consonants and vowels actually appear. Just as important:
  which ones *never* do. A world with no `z`, no `w`, and no `j` has a real constraint
  that a generic fantasy name generator will violate immediately.
- **Syllable shape.** CV, CVC, CVCC? Where do clusters occur — onset, coda, both?
  `Corrath`, `Marrow`, `Vashen`, `Aldric` are not the same shape as `Kaelthariel`.
- **Length.** Syllable count distribution. If every existing name is two syllables,
  three is a statement.
- **Recurring morphemes.** `-ath`, `-en`, `Vane`/`Vashen`. These may be meaningful —
  patronymics, place suffixes, house markers. Ask before reusing one; using a suffix
  that means "of the river" on an inland town is worse than a neutral name.
- **Orthographic habits.** Doubled consonants, apostrophes, accents. Whether the writer
  uses them at all is a style decision you must not overturn.

State the derivation before you produce a single name. It is the part the writer can
correct, and correcting it fixes every name at once.

## Then generate

Produce more than asked for — twelve for a request of three — grouped by how far each
sits from the core of the pattern:

- **Central**: could already be in the world.
- **Edge**: stretches one parameter deliberately, and say which.

Annotate each with what it is built from. `Ferrath — Fer- as in Ferrow, -ath as in
Corrath; reads as a place name, not a person.`

## Do not file these

Names are taste. Offer them; let the writer choose. Only after they pick should you
`propose_changes` to create the records — and then use the id convention already in the
world, not one you invented.
