# Open the images of Resources in the content, a drawn one from the window's memory

Status: Accepted

## Context

The details panel lists the images a session came across: image files, and
images its console drew, which only the window keeps. Everything else the
user opens goes into the content, where it can sit in a tab or a split
beside the session it belongs to.

## Decision

An image of Resources opens in the content as anything opened does. An image
file opens in its viewer. An image the console drew opens in the image viewer
as a thing of its own, read from the copy the window keeps in memory.

## Alternatives considered

- Show each image in a window of its own. It leaves the window's layout, and
  the user has to find and close each such window apart from the content.
- Write a drawn image to a file and open that file in its viewer. The copies
  would outlive the window, which keeping them in its memory avoids.

## Consequences

- A drawn image's window shows nothing once the window no longer keeps the
  image, and is not kept for the next run.
- The ways of opening a row of the tree open an image of Resources too.
