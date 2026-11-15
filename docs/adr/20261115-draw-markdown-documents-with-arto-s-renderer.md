# Draw Markdown documents with Arto's renderer

Status: Accepted

## Context

Agents write their plans, notes and READMEs as Markdown, and the user reads
them while the agent works. Opened as text in Neovim, a document shows its
markup rather than its diagrams, formulas, tables and alerts. Arto, a Markdown
reader by the same author, publishes its renderer for other applications to
embed: one crate turns Markdown into HTML and another carries the stylesheet
and scripts that draw what the HTML leaves to script.

## Decision

The window shows a Markdown document in a viewer of its own, drawn by Arto's
renderer: the HTML Arto's Markdown crate writes, in a frame apart from the
window, styled and drawn by Arto's frontend. Adroit takes the renderer with
its defaults and none of Arto's own configuration.

## Alternatives considered

- **A Markdown crate of Adroit's own choosing, styled by Adroit.** It would
  show the text, but diagrams, formulas and the colours of code would each be
  another library to choose, bundle and keep, and the document would read
  differently in Adroit and in Arto.
- **Arto's page renderer, which writes a whole page.** It reads Arto's
  configuration, so how a document looks in Adroit would follow settings made
  for another application.
- **Opening the document in Arto.** It takes the reader out of the window the
  agent's work is in, and needs Arto installed.

## Consequences

- A document reads in Adroit the way it reads in Arto, and changes with each
  release of Arto's renderer that Adroit takes.
- The frontend's scripts bundle libraries whose notices its crate does not
  carry, so Adroit keeps and ships them itself, and checks them on each bump.
- The renderer reads the file from this computer's disk, as every viewer
  does, so a work machine's documents stay in Neovim.
