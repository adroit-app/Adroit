# Start an embedded Neovim afresh once its configuration is saved, keeping what it shows

Status: Accepted

## Context

The prompt and the console's reader are Neovims that read a configuration of
the user's, which the user edits while they run. Reading a saved
configuration again inside the running Neovim sources it a second time, and
much of what configurations do cannot be done twice: plugin managers such as
lazy.nvim refuse to be set up again, and plugins keep state from the first
time. Users of them met errors and half-applied settings on every save. A
Neovim only reads a configuration as it was meant to be read when it starts.

## Decision

A Neovim whose configuration is saved tells the window, and the window starts
it again, as it does one that quits. What the user had in it is carried over:
the prompt's text and cursor, and where the reader was in what it shows.

## Alternatives considered

- Source the configuration again in the running Neovim, forgetting its Lua
  modules first. Configurations that cannot be read twice break, and nothing
  adroit does can make them safe to read twice.
- Leave a saved configuration to the next start. The prompt would have to be
  quit by hand, losing what is written in it, to see a change.

## Consequences

- Whatever a configuration does at start works the same on a save, and one
  that fails tells of it as one that fails at start does.
- A save costs a Neovim's start, and what is held only in the Neovim and
  not carried over is lost with it.
- The window has to be able to take what the user had out of one Neovim and
  put it into the next.
