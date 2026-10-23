#!/usr/bin/env bash
# Copy the file tree's icon tables from vim-nerdfont and vim-glyph-palette.
#
#   assets/file-icons/update.sh [NERDFONT_REV] [GLYPH_PALETTE_REV]
#
# Each revision defaults to the upstream's default branch. The commit copied
# is written to REVISION beside the files, so a build can be traced back to
# it and the next update starts from what is there.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/adroit-file-icons.XXXXXX")
trap 'rm -rf "$work"' EXIT

# fetch NAME REV FILE... copies FILE from lambdalisue/NAME at REV.
fetch() {
    local name=$1 rev=$2
    shift 2
    git clone --quiet "https://github.com/lambdalisue/$name" "$work/$name"
    if [[ -n $rev ]]; then
        git -C "$work/$name" checkout --quiet "$rev"
    fi
    rm -rf "${here:?}/$name"
    mkdir -p "$here/$name"
    local file
    for file in "$@"; do
        cp "$work/$name/$file" "$here/$name/$(basename "$file")"
    done
    git -C "$work/$name" rev-parse HEAD >"$here/$name/REVISION"
}

fetch vim-nerdfont "${1:-}" \
    LICENSE \
    LICENSE.vim-devicon \
    assets/json/basename.json \
    assets/json/directory.json \
    assets/json/extension.json \
    assets/json/pattern.json
fetch vim-glyph-palette "${2:-}" \
    LICENSE \
    autoload/glyph_palette/defaults.vim
