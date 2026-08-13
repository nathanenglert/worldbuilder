# Skills

Worldbuilding *methodology*, packaged so it can be distributed and extended without
shipping app updates. Each folder is a skill: a set of instructions an agent loads when
the task matches, describing how to use the [MCP server](../crates/wb-mcp) well.

The tools tell an agent what it *can* do. These tell it what is worth doing, and — more
often — what to refuse to do on the writer's behalf.

| Skill | For |
|---|---|
| [`consistency-audit`](consistency-audit/) | Triage contradictions, and tell a bug from a deliberate mystery |
| [`world-from-notes`](world-from-notes/) | Turn years of existing prose notes into records without inventing precision |
| [`chapter-canon-check`](chapter-canon-check/) | Check a draft against the world as it stood on that date |
| [`culture-from-phonology`](culture-from-phonology/) | Derive a sound system from existing names, then generate inside it |
| [`succession-crisis`](succession-crisis/) | Work out who claims a vacant title, and what follows |
| [`iceberg-check`](iceberg-check/) | Find what is overbuilt and unused, and what the story leans on that was never built |

## Installing

Copy the ones you want into your agent's skills folder. For Claude Code:

```sh
cp -r skills/consistency-audit ~/.claude/skills/
```

Or, per-project, into `.claude/skills/` beside the world.

They need the MCP server connected — see [the setup guide](../docs/mcp.md).

## The through-line

Every one of these ends the same way: **offer, do not decide.** A name is taste, a
mystery is a plot, and where to spend the next hour is the writer's call. The tools
enforce this structurally — nothing an agent does can reach canon without a human
accepting it — but a skill that pushes against that boundary all session is still a bad
experience, however safe it is.

## Writing your own

A skill is a folder with a `SKILL.md`: YAML frontmatter carrying `name` (matching the
folder) and `description`, then Markdown. The description is what decides whether the
skill gets loaded at the right moment, so it should say plainly *when to use this* — see
any of the six above.

`crates/wb-mcp/tests/skills.rs` checks that every tool the server exposes is mentioned by
at least one shipped skill, so a tool never ships without methodology attached.
