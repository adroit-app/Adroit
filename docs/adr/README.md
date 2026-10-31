# Architecture Decision Records

The significant decisions behind Adroit, one decision per file.

## Conventions

- Files are named `YYYYMMDD-<slug>.md` after the day the decision took the
  form the record states, so listing the directory in name order lists the
  decisions in time order. A record that merges others, or restates one, takes
  the day of its newest decision. Numbers and an index are avoided because
  parallel changes would collide on them.
- The slug is the title in kebab case, so a file name and its title always lead
  to each other.
- Each record has a status and the sections Context, Decision, Alternatives
  considered and Consequences.
- The directory holds the decisions in force, and nothing else. When a decision
  changes, its record is rewritten in place, from the title, as if it had been
  decided that way; when the title changes, the file is renamed with it. A
  decision that no longer holds is deleted. Git keeps the history.
- Records refer to each other by file name. A record does not link to the
  specification, which keeps changing; the decision is complete within the
  record.

## What belongs in a record

A record holds one decision, not everything decided while doing the work. It
should need no edit until the decision itself changes.

For every line, ask the replacement test: **if this changed, would the
decision have changed, and the record be rewritten from its title?**

- Yes — it is part of the decision. Keep it.
- No, an ordinary change to the code or the specification would do — it is
  specification or implementation. Leave it out.

Formats, file locations, encodings, setting and command names, default keys,
lists of supported kinds, UI details and the order of steps almost always
answer no. So do counts of things the project defines.

A change to the code or the specification that leaves a decision standing
leaves its record alone. If it would not, the record holds too much.

## How to write one

Write in this order, and do not judge details line by line along the way:

1. **Title** — the decision as one sentence. This sentence is the decision.
2. **Decision** — restate the title, with only what the replacement test above
   keeps.
3. **Alternatives considered** — only options that would have replaced the
   title's decision, each with why it was not taken.
4. **Consequences** — only what follows directly from the decision.
5. **Context** — only what a reader needs to see why the title was the answer.

When a record under review needs fixing, rewrite it from the title without
looking at the previous draft. Editing the draft keeps the details the new
decision no longer needs.

Before committing, apply the replacement test to every line once more.

## Example

This is the size to aim for.

```markdown
# Keep the user's words in a dictionary of their own

Status: Accepted

## Context

Words the user registers were appended to the bundled dictionary. Every update
replaces the bundled dictionary, so the user's words were lost.

## Decision

The user's words live in a separate dictionary that updates never touch.

## Alternatives considered

- Merge the user's words back into the bundled dictionary after each update.
  A failed merge still loses them, and the two sources become impossible to
  tell apart.

## Consequences

- Updates can replace the bundled dictionary wholesale.
- Lookups consult two dictionaries.
```
