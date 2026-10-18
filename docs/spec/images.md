# Images in the console

Programs in a session or a shell can show images with the
[kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/),
so tools that draw plots or previews in a terminal draw them in adroit too.
Claude Code draws its own pictures, such as diagrams a mod renders, in the
sessions adroit starts or takes over: they are told the console draws
images, which Claude Code otherwise believes only of kitty and Ghostty.

## What is understood

The protocol is understood as felis understands it, which follows kitty:
images sent in the escape sequence, in a file or in shared memory, at once or
in chunks, as PNG or raw pixels, compressed or not; put as they are sent or
later by id or number, whole or a part of them; put by placeholder
characters; moved with the lines they are on and taken away as kitty takes
them away. A program asking whether images work is answered.

Images are drawn over the text, the later ones above the earlier, and an
animation shows the frame the program last showed.
