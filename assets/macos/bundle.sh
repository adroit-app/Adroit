#!/usr/bin/env bash
# Assemble target/macos/Adroit.app from the release build: the executable,
# Info.plist with the workspace's version, the icon drawn from the logo, and
# the Neovim assets/neovim pins, which the prompt and the console's reader run.
# Signed, as macOS will not open an app bundle without a seal, by the identity
# ADROIT_SIGN_IDENTITY names, else by "Adroit Release" when the keychain has
# it, else ad hoc. Only the same identity keeps what macOS let the app do.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
target="${CARGO_TARGET_DIR:-$root/target}"
app="$target/macos/Adroit.app"
work="$target/macos/work"
version=$(perl -ne 'print $1 and exit if /^version = "(.*)"/' "$root/Cargo.toml")

rm -rf "$app" "$work"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$work/adroit.iconset"
cp "$target/release/adroit" "$app/Contents/MacOS/adroit"
# The build links libiconv from the Nix store; macOS has its own, so the app
# runs where the store is not.
nix_iconv=$(otool -L "$app/Contents/MacOS/adroit" | perl -ne 'print $1 if m{(\/nix\/store\/\S*libiconv\S*)}')
if [ -n "$nix_iconv" ]; then
  install_name_tool -change "$nix_iconv" /usr/lib/libiconv.2.dylib "$app/Contents/MacOS/adroit" 2>/dev/null
fi
cp "$root/assets/macos/Info.plist" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$app/Contents/Info.plist"

# The Neovim archive as it is released, which finds its lib/ and share/ beside
# bin/, with the licence it is shipped under.
neovim=$("$root/assets/neovim/fetch.sh")
mkdir -p "$app/Contents/Resources/neovim"
cp -R "$neovim/bin" "$neovim/lib" "$neovim/share" "$app/Contents/Resources/neovim/"
cp "$root/assets/neovim/LICENSE.txt" "$app/Contents/Resources/neovim/LICENSE.txt"

# Quick Look draws the SVG as WebKit would; iconutil wants each size.
qlmanage -t -s 1024 -o "$work" "$root/assets/logo/adroit-icon.svg" >/dev/null
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$work/adroit-icon.svg.png" --out "$work/adroit.iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$work/adroit-icon.svg.png" --out "$work/adroit.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$work/adroit.iconset" -o "$app/Contents/Resources/adroit.icns"

identity="${ADROIT_SIGN_IDENTITY:-}"
if [ -z "$identity" ]; then
  # An identity, not a certificate: the certificate alone, without its
  # private key, cannot sign. Captured before grepping: under `pipefail`,
  # `grep -q` closing the pipe early would fail `security` instead.
  identities="$(security find-identity -p codesigning)"
  if grep -qF '"Adroit Release"' <<<"$identities"; then
    identity="Adroit Release"
  else
    identity="-"
    echo "no \"Adroit Release\" in the keychain; signing ad hoc, so macOS will ask for its permissions again (just macos-make-identity, then just macos-import-identity)" >&2
  fi
fi
# The code the app carries is signed first: sealing the app does not sign
# what it holds, and a signature the app's does not match is refused.
while IFS= read -r -d '' code; do
  if [[ $(file -b "$code") == *Mach-O* ]]; then
    codesign --force --sign "$identity" "$code"
  fi
done < <(find "$app/Contents/Resources/neovim" -type f -print0)
codesign --force --sign "$identity" "$app"
rm -rf "$work"
echo "$app"
