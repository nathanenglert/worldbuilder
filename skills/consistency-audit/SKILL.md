---
name: consistency-audit
description: Audit a Worldbuilder world for contradictions, separating real errors from deliberate mysteries. Use when the writer asks to check their world, find plot holes, or review canon for problems.
---

# Consistency audit

The engine finds contradictions. It cannot tell a mistake from a mystery. That
judgement is the entire job here, and it is the one thing a deterministic rule will
never do.

## Run it

```
describe_world                 → the calendar, and where consistency stands
check_consistency              → every finding
```

Findings carry a **certainty**, and it decides how you treat them:

| | What it means | What to do |
|---|---|---|
| `definite` | Wrong under every reading of every fuzzy date. No date can be stretched to rescue it. | Treat as a defect. Propose a fix. |
| `possible` | The world's own vagueness leaves a reading where it is fine. | **Ask.** This is the shape a deliberate mystery takes. |

## The rule that matters

**A `possible` finding is not a to-do item.** A writer who dated a death `0811~` and
then put that character at a siege in 812 may have made a mistake — or may be writing a
story about whether he really died. Those are indistinguishable from the data, and
guessing wrong means proposing to delete the mystery at the centre of someone's book.

Before you say anything about a `possible` finding:

1. `get_entity` on the subject and read the **prose body**. Writers put the intent
   there. "Nobody found the body" is not an accident.
2. Check whether the vagueness is *load-bearing*. A date written `0811~` when every
   other date in the world is exact is a deliberate act.
3. Report it as a question with both readings spelled out, and say what each would cost.
   Do not file a proposal.

For `definite` findings, propose the fix — but propose the *smallest* one. A
contradiction between two facts can usually be resolved from either end, and which end
is wrong is the writer's call. Say which you picked and why.

## Before proposing anything

`list_proposals` first. A finding often already has a fix waiting in the queue — the
writer has seen it and not decided yet, and filing a second proposal for the same thing
buries the first. If one exists, say so and stop; the finding is already someone's
problem.

Then `check_changes`. It reports what your fix would settle **and what it would break**.
A fix that clears one definite finding and introduces two is not a fix.

```
check_changes  → resolves: [...]  introduces: [...]  breaks_something: true|false
```

If `breaks_something` is true, do not file it. Work out why first.

## What to hand back

Group by certainty, not by rule. Lead with definite findings, each with a proposed fix
and its measured impact. Then possible findings as questions, each naming the two
readings. Then, briefly, what is *clean* — an audit that only lists problems reads as
though the world is in worse shape than it is.

Do not pad the report with counts of rules that fired zero times.
