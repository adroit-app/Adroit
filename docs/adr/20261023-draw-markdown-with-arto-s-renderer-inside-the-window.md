# Draw Markdown with Arto's renderer inside the window

Status: Proposed

## Context

Markdown a session writes carries diagrams, formulas, alerts and code, and
links to other files of the same worktree. Its viewer has to draw all of that,
and to act on it: follow a link within adroit, take the reader from a block to
its line in Neovim, move by adroit's keys. Arto renders Markdown the way its
user reads it everywhere else, and splits that into a Rust pipeline,
`arto-markdown`, and a frontend that draws what the pipeline leaves to script.

## Decision

The Markdown viewer places the fragment `arto-markdown` renders, with what the
render reports of it, in the window's own document, and draws diagrams,
formulas and code with Arto's frontend. adroit acts on the fragment's links,
source ranges and images itself.

## Alternatives considered

- A self-contained page from `arto-page`, shown like any HTML page. adroit
  could not reach into it to follow a link or find a block's line.
- Hand the file to Arto's app. The document would leave the window and its
  layout.
- A Markdown renderer of adroit's own. It would draw the same document unlike
  everything else the user reads it in, and be one more renderer to keep.

## Consequences

- adroit depends on `arto-markdown` and on Arto's frontend being published for
  hosts, which Arto does not yet do.
- The fragment's markup and the frontend's script interface are an interface
  adroit relies on; a change to either in Arto may need adroit changed.
- What a document's raw HTML may do in the window is what `arto-markdown`
  lets through.
