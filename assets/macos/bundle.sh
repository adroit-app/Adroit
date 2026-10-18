#!/usr/bin/env bash
# Assemble target/macos/Adroit.app from the release build: the executable,
# Info.plist with the workspace's version, and the icon drawn from the logo.
# Signed ad hoc, as macOS will not open an app bundle without a seal.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
app="$root/target/macos/Adroit.app"
work="$root/target/macos/work"
version=$(perl -ne 'print $1 and exit if /^version = "(.*)"/' "$root/Cargo.toml")

rm -rf "$app" "$work"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$work/adroit.iconset"
cp "$root/target/release/adroit" "$app/Contents/MacOS/adroit"
# The build links libiconv from the Nix store; macOS has its own, so the app
# runs where the store is not.
nix_iconv=$(otool -L "$app/Contents/MacOS/adroit" | perl -ne 'print $1 if m{(\/nix\/store\/\S*libiconv\S*)}')
if [ -n "$nix_iconv" ]; then
  install_name_tool -change "$nix_iconv" /usr/lib/libiconv.2.dylib "$app/Contents/MacOS/adroit" 2>/dev/null
fi
cp "$root/assets/macos/Info.plist" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$app/Contents/Info.plist"

# Quick Look draws the SVG as WebKit would; iconutil wants each size.
qlmanage -t -s 1024 -o "$work" "$root/assets/logo/adroit-icon.svg" >/dev/null
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$work/adroit-icon.svg.png" --out "$work/adroit.iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$work/adroit-icon.svg.png" --out "$work/adroit.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$work/adroit.iconset" -o "$app/Contents/Resources/adroit.icns"

codesign --force --sign - "$app"
rm -rf "$work"
echo "$app"
