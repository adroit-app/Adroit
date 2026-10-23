# Viewers

A file that means nothing as text is shown by a viewer of its kind, drawn by
the window itself: images, movies, HTML pages and PDFs. Every other file,
including one of a kind adroit does not know, opens in Neovim.

Opening a file from the tree ([workspaces](workspaces.md#the-tree)) shows it
in the focused window, as opening any row does ([content](content.md)): in its
viewer, or in a Neovim started on it in its worktree, listed under Editors. A
file already open in a Neovim started on it opens that Neovim again. A viewer
runs in the window and ends with the windows showing it; it keeps its zoom,
page and playback while the focus moves between windows.

## Kinds

A file's kind is decided by its extension alone, whatever its case. The
common image formats open in the image viewer, the common movie formats in
the movie viewer, `.html` and `.htm` in the HTML viewer, and `.pdf` in the PDF
viewer.

## What each viewer does

- **Image** draws the picture whole within the window, and zooms in and out
  from there; zooming back to the fit shows it whole again.
- **Movie** plays and pauses the movie and moves back and forward a few
  seconds at a time. The time played and the movie's length are shown.
- **HTML** draws the page as a browser would, scripts included. It shows a
  file, never an address.
- **PDF** shows one page at a time, turned by keys, and zooms like an image.
  The page shown and the page count are shown.

In every viewer the content scrolls by lines, by half the window, and to its
top and bottom, and the file's path can be copied.

## What a viewer reaches

A viewer only reads its file, and reads nothing else, with one exception: an
HTML page also reads the files under its own directory, which it links to by
relative paths. A page cannot reach files outside that directory, nor the
window it is drawn in, nor adroit.

## Keys

A viewer's keys are actions bound in the keys file like any other, in its
`viewer` area ([format](keys.md)): scrolling (`scroll.*`), zooming (`viewer.zoom_in`,
`viewer.zoom_out`, `viewer.zoom_fit`), turning pages (`viewer.page_next`,
`viewer.page_prev`), playing (`viewer.play`, `viewer.back`, `viewer.forward`),
copying the path (`viewer.copy_path`) and closing its window
(`window.quit`). They reach the viewer of the focused window alone. Keys
pressed while an HTML page has the focus reach the viewer as well, except
those typed into the page's own fields.
