---
id: act_aldric_vane
name: Aldric Vane
# The book calls him Aldric and, on the wall, "the duke". Without these the iceberg would
# report him as never reaching the page, which is the opposite of true.
aka: [Aldric, the duke]
type: noble
existence: { from: "0771-06-12", to: "0811~" }
parents: [act_maren_vane, act_isolde_corr]
facts:
  - attr: title
    value: "Duke of Corrath"
    from: "0799-01-01"
    to: "@act_aldric_vane.death"
  - attr: seat
    value: place_corrath_city
    from: "0799-01-01"
    to: "@evt_oath_of_vashen"
  - attr: seat
    value: place_marrow
    from: "@evt_oath_of_vashen"
    to: "@act_aldric_vane.death"
  - attr: allegiance
    value: pol_corrath
    from: "@act_aldric_vane.birth"
    to: "@evt_oath_of_vashen"
  - attr: allegiance
    value: pol_vashen
    from: "@evt_oath_of_vashen"
    to: "@act_aldric_vane.death"
---

Fourth of his name and last of the line to hold the Vale outright. Aldric took the
ducal seat at twenty-eight and spent the next seven years failing to keep Corrath out
of Vashen's reach.

The date of his death is not recorded. The Marrow chronicle places it "in the winter
after the ice broke early", which the chapterhouse reckoned as 811 — but the same
chronicle has him at the walls during the siege, a year later. Both cannot be true,
and the world does not yet know which is wrong.
