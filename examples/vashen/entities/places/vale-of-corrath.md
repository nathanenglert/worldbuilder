---
id: ter_vale_of_corrath
name: The Vale of Corrath
# "the Vale" and nothing shorter. A bare "vale" would match the common noun, and a wrong
# iceberg ratio is worse than a conservative one.
aka: [the Vale]
type: region
shape:
  - [0.18, 0.34]
  - [0.34, 0.27]
  - [0.45, 0.37]
  - [0.42, 0.56]
  - [0.24, 0.59]
  - [0.14, 0.47]
facts:
  - attr: owner
    value: pol_corrath
    from: "@evt_founding_of_corrath"
    to: "@evt_siege_of_marrow"
  - attr: owner
    value: pol_vashen
    from: "@evt_siege_of_marrow"
---

The territory itself, as distinct from the duchy that held it. One attribute — `owner`
— carried by two intervals that meet at the siege. Scrub across Verdant 812 and the
border changes hands; scrub *within* that month and neither claim is settled, because
nobody wrote down the day.
