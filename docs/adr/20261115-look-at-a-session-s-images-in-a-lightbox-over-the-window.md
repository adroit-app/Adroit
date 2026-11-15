# Look at a session's images in a lightbox over the window

Status: Accepted

## Context

A session's console draws images and its files hold more, and they are
listed together in the details panel. Opening one shows it in a window of
the content. With no preview tab, each image looked at in passing would
leave a tab behind. Going through a session's images one after another would
mean going back to the panel for every one.

## Decision

An image a session drew or wrote is looked at in a lightbox over the window.
The lightbox steps through that session's images in place, and closes without
leaving anything behind. An image is shown in a window of the content only
when it is opened there on purpose, in a split or a new tab.

## Alternatives considered

- Open every image in a tab, as any file is. Every image looked at would
  leave a tab to close, and stepping through them would take a trip to the
  panel each time.
- Keep a preview tab for images alone. It would bring back a tab that
  changes under the user, which had been given up for everything else.

## Consequences

- The lightbox holds the keys while it is open, by bindings of its own.
- An image file opened from the tree still opens in a tab; only a session's
  images are looked at in the lightbox.
