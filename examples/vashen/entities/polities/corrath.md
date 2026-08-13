---
id: pol_corrath
name: The Duchy of Corrath
type: duchy
existence: { from: "@evt_founding_of_corrath", to: "@evt_siege_of_marrow" }
facts:
  # Dateless, so it holds for every moment the duchy exists.
  - attr: color
    value: "#B07A2B"
  - attr: capital
    value: place_corrath_city
    from: "@evt_founding_of_corrath"
    to: "@evt_oath_of_vashen"
  - attr: capital
    value: place_marrow
    from: "@evt_oath_of_vashen"
    to: "@evt_siege_of_marrow"
---

Three hundred years of independence between the founding and the siege, most of it
spent paying someone not to invade.
