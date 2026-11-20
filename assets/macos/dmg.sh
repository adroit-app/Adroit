#!/usr/bin/env bash
# Put the Adroit.app bundle.sh assembled into the disk image a GitHub Release
# carries and the Homebrew cask installs, beside a link to /Applications:
#
#   dmg.sh <version> <dist>
#
# The release runs this with the tag's version; CI runs it with a
# placeholder, so a release is never its first run.
set -euo pipefail
version=$1
dist=$2
root=$(cd "$(dirname "$0")/../.." && pwd)
target="${CARGO_TARGET_DIR:-$root/target}"
app="$target/macos/Adroit.app"

# A build made in the Nix development shell may refer into /nix/store, which
# no machine without that store has; it must not be what a release ships.
linked=$(otool -L "$app/Contents/MacOS/adroit")
if grep -q /nix/store <<<"$linked"; then
  echo "error: Adroit.app refers into /nix/store; build it without the Nix shell" >&2
  echo "$linked" >&2
  exit 1
fi

case "$(uname -m)" in
arm64 | aarch64) arch=aarch64 ;;
*) arch=$(uname -m) ;;
esac

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/Adroit" "$dist"
# ditto keeps the signatures and the symbolic links Neovim's folders hold.
ditto "$app" "$stage/Adroit/Adroit.app"
ln -s /Applications "$stage/Adroit/Applications"
# hdiutil create fails now and then on GitHub's macOS runners while the
# diskimages-helper of an earlier image still holds the device ("Resource
# busy"), and succeeds when run again a moment later.
for attempt in 1 2 3; do
  if hdiutil create -volname Adroit -srcfolder "$stage/Adroit" -ov \
    -format UDZO "$dist/Adroit_${version}_${arch}.dmg"; then
    break
  fi
  if [ "$attempt" = 3 ]; then
    echo "error: hdiutil create failed $attempt times" >&2
    exit 1
  fi
  echo "hdiutil create failed; trying again" >&2
  sleep $((attempt * 10))
done
ls -l "$dist"
