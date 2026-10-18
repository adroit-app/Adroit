# Recall the prompts Claude Code keeps

Status: Accepted

## Context

A prompt sent before is often sent again with a change. Claude Code recalls the
prompts sent in a directory with its Up arrow, from a history it keeps for every
session, those started outside adroit and before the window opened included.
Prompts are written in adroit's prompt, so Claude Code's own recall does not
reach where they are written.

## Decision

The prompt recalls the prompts Claude Code keeps for the directory the shown tab
works in, read from Claude Code's history when they are asked for.

## Alternatives considered

- Keep a history of the prompts adroit sends. It would miss every prompt sent
  outside adroit or before the window opened, and it would be a record of
  adroit's own about sessions.
- Drive Claude Code's Up arrow and take its input back through the external
  editor. Each step would be a round trip through the editor, and taking the
  input back sets Claude Code's recall back to the newest.

## Consequences

- Claude Code's history file is not a published format; a change to it breaks
  recall until adroit follows.
- A shell has no Claude Code history, and recalls nothing.
