# Ship JetBrainsMono Nerd Font Mono and draw the console in it by default

Status: Accepted

## Context

Shell prompts, status lines, Neovim plugins and the file tree's icons use Nerd
Font glyphs, which sit in Unicode's private use area. The system's monospace
fonts draw none of them, so without a Nerd Font installed and named they show
as missing characters.

A Nerd Font is an existing font patched with the glyphs. Whether it can be
shipped depends on the original font's licence: under the SIL Open Font
License a font may be bundled with software beside its licence, but a modified
font may not keep a Reserved Font Name. JetBrains Mono is licensed under
OFL-1.1 with no Reserved Font Name, and Nerd Fonts publishes it patched under
its own name.

## Decision

Adroit ships JetBrainsMono Nerd Font Mono from a pinned Nerd Fonts release,
built into the binary with its licence beside Adroit's, and draws the console,
the prompt and the file tree's glyphs in it unless the user picks another
font. A font the user picks is followed by it, so the glyphs still draw.

## Alternatives considered

- Hack Nerd Font. Hack derives from Bitstream Vera, whose licence reserves the
  names Bitstream and Vera; the patch keeps clear of them, but the licence is
  a pair of licences where JetBrains Mono's is one.
- Fira Code Nerd Font. Also OFL-1.1 without a Reserved Font Name, but it stops
  at a lighter range of weights, and its ligatures are its point, which a
  terminal grid does not draw.
- Keep the system's fonts and ship only Nerd Fonts' symbols font as a fallback
  for the glyphs. The glyphs would draw, but in a size and placement made for
  no particular font, and the default look would still differ from machine to
  machine.
- Ask the user to install a Nerd Font. Every new user meets missing characters
  first.

## Consequences

- The binary grows by each style shipped, so only the styles the console draws
  ordinary and bold text in are shipped; other weights are drawn in the
  nearest shipped one.
- One font serves both the console and the file tree.
- The default look is the same on every machine.
