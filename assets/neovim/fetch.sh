#!/usr/bin/env bash
# Fetch the Neovim VERSION pins, for this machine's system and architecture,
# check it against SHA256SUMS, and print the folder it is unpacked in, which
# holds bin/nvim beside the lib/ and share/ it finds at run time. Kept under
# the target folder, so a second run fetches nothing.
#
#   assets/neovim/fetch.sh
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
tag=$(cat "$here/VERSION")
case "$(uname -s)" in
Darwin) system=macos ;;
Linux) system=linux ;;
*) echo "no Neovim archive for $(uname -s)" >&2 && exit 1 ;;
esac
case "$(uname -m)" in
arm64 | aarch64) arch=arm64 ;;
x86_64) arch=x86_64 ;;
*) echo "no Neovim archive for $(uname -m)" >&2 && exit 1 ;;
esac
archive="nvim-$system-$arch.tar.gz"
want=$(perl -ne "print \$1 if /^([0-9a-f]{64})  \Q$archive\E\$/" "$here/SHA256SUMS")
if [ -z "$want" ]; then
    echo "no SHA-256 for $archive in SHA256SUMS" >&2
    exit 1
fi

cache="${CARGO_TARGET_DIR:-$root/target}/neovim/$tag"
unpacked="$cache/nvim-$system-$arch"
if [ ! -x "$unpacked/bin/nvim" ]; then
    mkdir -p "$cache"
    curl -fsSL -o "$cache/$archive" "https://github.com/neovim/neovim/releases/download/$tag/$archive"
    got=$(shasum -a 256 "$cache/$archive" | cut -d' ' -f1)
    if [ "$got" != "$want" ]; then
        rm -f "$cache/$archive"
        echo "$archive is $got, not the $want SHA256SUMS pins" >&2
        exit 1
    fi
    rm -rf "$unpacked"
    tar -C "$cache" -xzf "$cache/$archive"
    rm -f "$cache/$archive"
fi
echo "$unpacked"
