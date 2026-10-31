# Draw the window with Dioxus desktop

Status: Accepted

## Context

Adroit needs one window that holds a tree, several terminal panes, an embedded
Neovim and a rich completion popup, and it is meant to run on macOS first and
on Linux and Windows later. A terminal UI cannot draw Claude Code's images
without re-encoding them for the outer terminal, and a native toolkit per
platform would be written more than once.

## Decision

The window is a Dioxus desktop application: Rust holds the state and talks to
the processes, and a webview draws the interface.

## Alternatives considered

- A terminal UI hosting the panes. Images from Claude Code would have to be
  relayed to the outer terminal, and completion menus are limited to the cell
  grid.
- A native toolkit per platform, such as AppKit. Every platform after the first
  would be a rewrite.
- Electron or Tauri with a JavaScript application. The terminal clients and
  Neovim's live in Rust anyway, and Dioxus keeps the interface in the same
  language.

## Consequences

- Terminal grids and the Neovim grid are drawn by the webview from cell data,
  not by a native terminal widget.
- Text input, including input methods, arrives through the webview.
- Linux builds depend on WebKitGTK.
