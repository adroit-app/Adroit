#!/usr/bin/env bash
# Copy the font the console, the prompt and the file tree's glyphs are drawn
# in from a Nerd Fonts release's JetBrainsMono Nerd Font Mono, in the styles
# the console draws, packed as WOFF2 to keep the binary small.
#
#   assets/fonts/update.sh [TAG]
#
# The tag defaults to the latest release. The tag copied is written to
# VERSION beside the fonts, so a build can be traced back to it.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/adroit-fonts.XXXXXX")
trap 'rm -rf "$work"' EXIT

tag=${1:-$(curl -fsSL https://api.github.com/repos/ryanoasis/nerd-fonts/releases/latest |
    perl -ne 'print $1 if /"tag_name":\s*"([^"]+)"/')}
curl -fsSL -o "$work/font.tar.xz" \
    "https://github.com/ryanoasis/nerd-fonts/releases/download/$tag/JetBrainsMono.tar.xz"
tar -C "$work" -xf "$work/font.tar.xz"

rm -f "$here"/*.ttf "$here"/*.woff2
for style in Regular Bold Italic BoldItalic; do
    uvx --from fonttools --with brotli fonttools ttLib.woff2 compress \
        -o "$here/JetBrainsMonoNerdFontMono-$style.woff2" \
        "$work/JetBrainsMonoNerdFontMono-$style.ttf"
done
# JetBrains Mono's licence, which the patched font keeps, and the release's
# notes, which name every icon set merged into it and its licence.
cp "$work/OFL.txt" "$here/OFL.txt"
cp "$work/README.md" "$here/README.md"
# Nerd Fonts' own licence, for the patcher and the glyphs it made.
curl -fsSL -o "$here/LICENSE" \
    "https://raw.githubusercontent.com/ryanoasis/nerd-fonts/$tag/LICENSE"
echo "$tag" >"$here/VERSION"
