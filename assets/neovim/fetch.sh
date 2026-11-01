#!/usr/bin/env bash
# Fetch the Neovim VERSION pins, for this machine's system and architecture,
# check it against SHA256SUMS, and print the folder it is unpacked in, which
# holds bin/nvim beside the lib/ and share/ it finds at run time. Kept under
# the target folder, so a second run fetches nothing. On Windows, run it in
# Git Bash; the folder is printed as Windows programs take it.
#
#   assets/neovim/fetch.sh
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
tag=$(cat "$here/VERSION")
case "$(uname -s)" in
Darwin) system=macos ;;
Linux) system=linux ;;
MINGW* | MSYS*) system=win ;;
*) echo "no Neovim archive for $(uname -s)" >&2 && exit 1 ;;
esac
case "$(uname -m)" in
arm64 | aarch64) arch=arm64 ;;
x86_64) arch=x86_64 ;;
*) echo "no Neovim archive for $(uname -m)" >&2 && exit 1 ;;
esac
# Windows releases are zip archives, the x86_64 one named win64.
case "$system-$arch" in
win-arm64) name=nvim-win-arm64 archive=nvim-win-arm64.zip program=nvim.exe ;;
win-x86_64) name=nvim-win64 archive=nvim-win64.zip program=nvim.exe ;;
*) name="nvim-$system-$arch" archive="nvim-$system-$arch.tar.gz" program=nvim ;;
esac
want=$(perl -ne "print \$1 if /^([0-9a-f]{64})  \Q$archive\E\$/" "$here/SHA256SUMS")
if [ -z "$want" ]; then
    echo "no SHA-256 for $archive in SHA256SUMS" >&2
    exit 1
fi

cache="${CARGO_TARGET_DIR:-$root/target}/neovim/$tag"
unpacked="$cache/$name"
if [ ! -x "$unpacked/bin/$program" ]; then
    mkdir -p "$cache"
    curl -fsSL -o "$cache/$archive" "https://github.com/neovim/neovim/releases/download/$tag/$archive"
    # Read from stdin, as sha256sum escapes a name with a backslash in it.
    got=$( (sha256sum 2>/dev/null || shasum -a 256) <"$cache/$archive")
    got=${got%% *}
    if [ "$got" != "$want" ]; then
        rm -f "$cache/$archive"
        echo "$archive is $got, not the $want SHA256SUMS pins" >&2
        exit 1
    fi
    rm -rf "$unpacked"
    if [ "$system" = win ]; then
        # Git Bash's tar reads no zip; the one Windows carries does.
        "$(cygpath -u "$SYSTEMROOT")/System32/tar.exe" -C "$cache" -xf "$cache/$archive"
    else
        tar -C "$cache" -xzf "$cache/$archive"
    fi
    rm -f "$cache/$archive"
fi
if [ "$system" = win ]; then
    cygpath -m "$unpacked"
else
    echo "$unpacked"
fi
