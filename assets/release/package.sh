#!/usr/bin/env bash
# Put the release build into the archive a GitHub Release carries for this
# system: a zip on Windows, run from Git Bash, a gzipped tarball on Linux.
#
#   package.sh <version> <dist>
#
# The archive unpacks into one folder named as the archive is, holding the
# executables, the Neovim assets/neovim pins in neovim/, which adroit runs
# from beside itself, and the licences every copy is to carry. The release
# runs this with the tag's version; CI runs it with a placeholder, so a
# release is never its first run.
set -euo pipefail
version=$1
dist=$2
root=$(cd "$(dirname "$0")/../.." && pwd)
target="${CARGO_TARGET_DIR:-$root/target}"

case "$(uname -s)" in
Linux) system=linux suffix= ;;
MINGW* | MSYS*) system=windows suffix=.exe ;;
*) echo "no release archive for $(uname -s)" >&2 && exit 1 ;;
esac
case "$(uname -m)" in
arm64 | aarch64) arch=aarch64 ;;
*) arch=$(uname -m) ;;
esac

if [ "$system" = linux ]; then
  # A build made in the Nix development shell asks for a loader in
  # /nix/store, which no machine without that store has.
  if readelf -l "$target/release/adroit" | grep -q /nix/store; then
    echo "error: adroit refers into /nix/store; build it without the Nix shell" >&2
    exit 1
  fi
fi

name="Adroit_${version}_${arch}-${system}"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
folder="$stage/$name"
mkdir -p "$folder" "$dist"

cp "$target/release/adroit$suffix" "$folder/"
if [ "$system" = windows ]; then
  # Started from a shortcut, it opens the window with no console showing.
  cp "$target/release/adroitw.exe" "$folder/"
fi

# The Neovim archive as it is released, which finds its lib/ and share/ beside
# bin/, with the licence it is shipped under.
neovim=$("$root/assets/neovim/fetch.sh")
mkdir -p "$folder/neovim"
cp -R "$neovim/bin" "$neovim/lib" "$neovim/share" "$folder/neovim/"
cp "$root/assets/neovim/LICENSE.txt" "$folder/neovim/LICENSE.txt"

# Adroit's licence, and those of the crates built into it, whose notices are
# to travel with every copy, and of the packages Arto's frontend bundles,
# which its crate carries no notices of.
mkdir -p "$folder/licenses/arto-frontend"
cp "$root/LICENSE" "$root/THIRD-PARTY-LICENSES.txt" "$folder/licenses/"
cp "$root"/assets/arto-frontend/*.LICENSE.txt "$folder/licenses/arto-frontend/"

dist=$(cd "$dist" && pwd)
if [ "$system" = windows ]; then
  # Git Bash has no zip; the tar Windows carries writes one.
  "$(cygpath -u "$SYSTEMROOT")/System32/tar.exe" -a -c \
    -f "$(cygpath -w "$dist/$name.zip")" -C "$(cygpath -w "$stage")" "$name"
else
  tar -C "$stage" -czf "$dist/$name.tar.gz" "$name"
fi
ls -l "$dist"
