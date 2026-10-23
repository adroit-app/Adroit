#!/usr/bin/env bash
# Cut the font the file tree's glyphs are drawn in from a Nerd Fonts
# release's Symbols Nerd Font Mono, keeping only the glyphs vim-nerdfont's
# tables in assets/file-icons/ use. Run it again after those tables change.
#
#   assets/fonts/update.sh [TAG]
#
# The tag defaults to the latest release. The tag copied is written to
# VERSION beside the font, so a build can be traced back to it.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
tables="$here/../file-icons/vim-nerdfont"
work=$(mktemp -d "${TMPDIR:-/tmp}/adroit-fonts.XXXXXX")
trap 'rm -rf "$work"' EXIT

tag=${1:-$(curl -fsSL https://api.github.com/repos/ryanoasis/nerd-fonts/releases/latest |
    perl -ne 'print $1 if /"tag_name":\s*"([^"]+)"/')}
curl -fsSL -o "$work/symbols.tar.xz" \
    "https://github.com/ryanoasis/nerd-fonts/releases/download/$tag/NerdFontsSymbolsOnly.tar.xz"
tar -C "$work" -xf "$work/symbols.tar.xz"

# The private-use characters the tables hold, plus vim-nerdfont's default.
perl -CSD -0777 -ne 'printf "U+%04X\n", ord $1 while /([\x{e000}-\x{f8ff}\x{f0000}-\x{10ffff}])/g' \
    "$tables"/*.json >"$work/unicodes"
echo U+F15B >>"$work/unicodes"

uvx --from fonttools pyftsubset "$work/SymbolsNerdFontMono-Regular.ttf" \
    --unicodes-file="$work/unicodes" \
    --output-file="$here/SymbolsNerdFontMono-Regular.ttf"
cp "$work/LICENSE" "$here/LICENSE"
echo "$tag" >"$here/VERSION"
