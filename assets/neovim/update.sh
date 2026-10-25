#!/usr/bin/env bash
# Pin the Neovim the macOS app carries, and the tests run in, to a release:
# its tag in VERSION and the SHA-256 of each archive fetch.sh may take in
# SHA256SUMS, as the release on GitHub states them.
#
#   assets/neovim/update.sh [TAG]
#
# The tag defaults to the latest release. Run the tests in it before
# shipping it: just test-neovim "$(assets/neovim/fetch.sh)/bin/nvim"
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
api=https://api.github.com/repos/neovim/neovim/releases
tag=${1:-$(curl -fsSL "$api/latest" | perl -ne 'print $1 if /"tag_name":\s*"([^"]+)"/')}
curl -fsSL "$api/tags/$tag" | perl -0ne '
    while (/"name":\s*"(nvim-(?:macos|linux)-[^"]+\.tar\.gz)".*?"digest":\s*"sha256:([0-9a-f]{64})"/gs) {
        print "$2  $1\n";
    }' >"$here/SHA256SUMS.new"
if [ ! -s "$here/SHA256SUMS.new" ]; then
    rm -f "$here/SHA256SUMS.new"
    echo "no archives with digests in Neovim $tag" >&2
    exit 1
fi
mv "$here/SHA256SUMS.new" "$here/SHA256SUMS"
# Neovim's licence, Apache 2.0 with the Vim licence for what came from Vim,
# which the app carries beside it.
curl -fsSL -o "$here/LICENSE.txt" "https://raw.githubusercontent.com/neovim/neovim/$tag/LICENSE.txt"
echo "$tag" >"$here/VERSION"
